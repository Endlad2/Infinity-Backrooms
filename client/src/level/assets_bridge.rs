//! Мост между декларациями ассетов в XML уровня и assets-резолвером.
//! Проходит по `Resources` и предварительно собирает готовые ассеты
//! (PNG-текстуры, OBJ-текст) — до построения сцены.
//!
//! Логика приоритета (§6.5, §6.7 ТЗ):
//!   1. src=/path=/url= — читаем файл/URL.
//!   2. color= — генерируем однотонную PNG.
//!   3. inline <svg>/<obj> — рендерим/парсим.
//!   4. tags=... — резолвим через assets.db / ИИ (см. assets::resolver).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::assets::resolver::{is_png, parse_tags, Resolver};
use crate::assets::svg;

use super::model::*;

/// Готовые к использованию ассеты уровня.
#[derive(Debug, Default)]
pub struct ResolvedAssets {
    /// PNG-байты текстур по id.
    pub textures: BTreeMap<String, Vec<u8>>,
    /// OBJ-текст (или иные байты) моделей по id.
    pub models: BTreeMap<String, Vec<u8>>,
    /// Источник каждого ассета: "src" | "db" | "ai" | "inline" | "color".
    pub origins: BTreeMap<String, &'static str>,
}

/// Пройти по ресурсам уровня и подготовить все текстуры/модели.
pub fn resolve_level_assets(
    level: &Level,
    resolver: &Resolver,
    assets_root: &Path,
) -> Result<ResolvedAssets> {
    let mut out = ResolvedAssets::default();

    // --- текстуры ---
    for tex in &level.resources.textures {
        let bytes_opt: Option<(Vec<u8>, &'static str)> = match &tex.source {
            TextureSource::File(path) => {
                let p = assets_root.join(path);
                let p2 = assets_root.join(path.strip_prefix("assets/").unwrap_or(path));
                let chosen = if p.is_file() { p } else { p2 };
                std::fs::read(&chosen).ok().map(|b| (b, "src"))
            }
            TextureSource::Url(_u) => {
                // Сетевые URL не тянем в этой версии — fallback в ИИ.
                None
            }
            TextureSource::Color(color) => {
                let svg_str = svg::solid_color_svg(color, tex.width.max(1), tex.height.max(1));
                svg::svg_to_png(&svg_str, tex.width.max(1), tex.height.max(1))
                    .ok()
                    .map(|b| (b, "color"))
            }
            TextureSource::InlineSvg(code) => svg::svg_to_png(code, tex.width, tex.height)
                .ok()
                .map(|b| (b, "inline")),
            TextureSource::Tags(_) | TextureSource::None => None,
        };

        if let Some((bytes, origin)) = bytes_opt {
            if is_png(&bytes) || !bytes.is_empty() {
                out.textures.insert(tex.id.clone(), bytes);
                out.origins.insert(tex.id.clone(), origin);
                continue;
            }
        }

        // Резолвер — теги/ИИ/БД или fallback-заглушка
        let res = resolver.resolve_texture(
            None,
            &tex.tags,
            "",
            tex.width.max(tex.height).max(64),
        )?;
        out.textures.insert(tex.id.clone(), res.bytes);
        out.origins.insert(tex.id.clone(), res.source);
    }

    // --- модели ---
    for m in &level.resources.models {
        let bytes_opt: Option<(Vec<u8>, &'static str)> = match &m.source {
            ModelSource::File(path) => {
                let p = assets_root.join(path);
                let p2 = assets_root.join(path.strip_prefix("assets/").unwrap_or(path));
                let chosen = if p.is_file() { p } else { p2 };
                std::fs::read(&chosen).ok().map(|b| (b, "src"))
            }
            ModelSource::Url(_) => None,
            ModelSource::InlineObj(code) => Some((code.as_bytes().to_vec(), "inline")),
            ModelSource::Tags(_) | ModelSource::None => None,
        };

        if let Some((bytes, origin)) = bytes_opt {
            out.models.insert(m.id.clone(), bytes);
            out.origins.insert(m.id.clone(), origin);
            continue;
        }

        let res = resolver.resolve_model(None, &m.tags, "")?;
        out.models.insert(m.id.clone(), res.bytes);
        out.origins.insert(m.id.clone(), res.source);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mk_level_with_assets() -> Level {
        let mut l = Level::default();
        l.resources.textures.push(TextureDecl {
            id: "tex_red".into(),
            source: TextureSource::Color("#ff0000".into()),
            tags: vec![],
            width: 32,
            height: 32,
            filter: None,
            wrap: None,
        });
        l.resources.textures.push(TextureDecl {
            id: "tex_wall".into(),
            source: TextureSource::Tags(vec!["wall".into(), "yellow".into()]),
            tags: vec!["wall".into(), "yellow".into()],
            width: 32,
            height: 32,
            filter: None,
            wrap: None,
        });
        l.resources.models.push(ModelDecl {
            id: "mdl_cube".into(),
            source: ModelSource::InlineObj("o cube\nv 0 0 0\nf 1 1 1\n".into()),
            tags: vec![],
        });
        l
    }

    #[test]
    fn resolves_color_texture_as_png() {
        let dir = tempdir().unwrap();
        let assets_root = dir.path().join("assets");
        std::fs::create_dir_all(&assets_root).unwrap();
        let db_path = dir.path().join("assets.db");
        let r = Resolver::open(&db_path, &assets_root).unwrap();

        let l = mk_level_with_assets();
        let out = resolve_level_assets(&l, &r, &assets_root).unwrap();
        assert!(out.textures.contains_key("tex_red"));
        assert!(is_png(&out.textures["tex_red"]));
        assert_eq!(out.origins["tex_red"], "color");
    }

    #[test]
    fn resolves_tags_texture_via_ai_fallback() {
        let dir = tempdir().unwrap();
        let assets_root = dir.path().join("assets");
        std::fs::create_dir_all(&assets_root).unwrap();
        let db_path = dir.path().join("assets.db");
        let r = Resolver::open(&db_path, &assets_root).unwrap();

        let l = mk_level_with_assets();
        let out = resolve_level_assets(&l, &r, &assets_root).unwrap();
        assert!(out.textures.contains_key("tex_wall"));
        // источник — "ai" (сгенерирована) либо "db" (уже закэширована)
        assert!(matches!(out.origins["tex_wall"], "ai" | "db"));
    }

    #[test]
    fn resolves_inline_obj_model() {
        let dir = tempdir().unwrap();
        let assets_root = dir.path().join("assets");
        std::fs::create_dir_all(&assets_root).unwrap();
        let db_path = dir.path().join("assets.db");
        let r = Resolver::open(&db_path, &assets_root).unwrap();

        let l = mk_level_with_assets();
        let out = resolve_level_assets(&l, &r, &assets_root).unwrap();
        let bytes = out.models.get("mdl_cube").unwrap();
        let text = String::from_utf8_lossy(bytes);
        assert!(text.contains("o cube"));
        assert_eq!(out.origins["mdl_cube"], "inline");
    }
}