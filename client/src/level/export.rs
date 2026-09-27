//! Экспорт уровня в .zip: XML + все использованные ассеты из cache-files/.
//!
//! Что делает `export_level_zip`:
//!   1. Читает levels/{N}.xml.
//!   2. Парсит его и находит все `src="assets/textures/*"` / `path="assets/models/*"`.
//!   3. Для каждой ссылки:
//!        - если файл в cache-files/ — копирует;
//!        - для моделей — копирует всю папку X_files/ (gltf + bin + textures).
//!   4. Складывает в zip: level.xml + assets/textures/* + assets/models/*_files/*.
//!   5. Возвращает путь к готовому zip (в %TEMP%).
//!
//! Импорт `import_level_zip`:
//!   1. Распаковывает zip во временную папку.
//!   2. level.xml → %APPDATA%/.infinity-backrooms/levels/{N}.xml.
//!   3. assets/textures/* → cache-files/*.
//!   4. assets/models/*_files/* → cache-files/*_files/*.
//!
//! Формат zip:
//!   level.xml
//!   manifest.json  (опционально: level_number, texture_res, source)
//!   assets/textures/<file>
//!   assets/models/<X_files>/<file>

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::level::parse::parse_level_xml;
use crate::paths::AppPaths;

/// Экспортирует уровень в zip в директорию `%TEMP%`.
/// Возвращает путь к готовому архиву.
pub fn export_level_zip(paths: &AppPaths, level_number: u32) -> Result<PathBuf> {
    let level_xml = paths.level_xml(level_number);
    if !level_xml.is_file() {
        return Err(anyhow!("levels/{}.xml не найден", level_number));
    }

    let xml_text = fs::read_to_string(&level_xml)?;
    let level = parse_level_xml(&xml_text)?;

    // Находим все src=/path= в <resources>.
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
    // Также inline <material texture= normal= .../> ссылается на texture id,
    // который уже объявлен в <resources>, — так что повторно не ищем.

    // Готовим zip.
    let tmp_dir = std::env::temp_dir().join("infinity-backrooms-export");
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    fs::create_dir_all(&tmp_dir)?;

    // level.xml
    fs::write(tmp_dir.join("level.xml"), &xml_text)?;

    // manifest.json
    let manifest = serde_json::json!({
        "level_number": level_number,
        "level_id": level.id,
        "level_name": level.name,
        "exported_by": "backrooms-client",
        "version": env!("CARGO_PKG_VERSION"),
    });
    fs::write(
        tmp_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // Ассеты.
    let mut copied: Vec<String> = Vec::new();
    for r in &refs {
        // Нормализуем путь: "assets/textures/X.png" → "X.png".
        let bare = r
            .strip_prefix("assets/textures/")
            .or_else(|| r.strip_prefix("assets/models/"))
            .or_else(|| r.strip_prefix("assets/"))
            .or_else(|| r.strip_prefix("cache-files/"))
            .unwrap_or(r);

        // 1) Текстура или модель прямо в cache-files/.
        let in_cache = paths.cache_files_dir.join(bare);
        if in_cache.is_file() {
            let target = tmp_dir.join("assets").join("textures").join(bare);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(&in_cache, &target)?;
            copied.push(format!("assets/textures/{bare}"));
            continue;
        }

        // 2) Модель в подпапке X_files/.
        if let Some(dir) = find_model_dir(&paths.cache_files_dir, bare) {
            let target_dir = tmp_dir.join("assets").join("models").join(
                dir.file_name().unwrap_or_default(),
            );
            copy_dir_recursive(&dir, &target_dir)?;
            copied.push(format!("assets/models/{}/", dir.file_name().unwrap_or_default().to_string_lossy()));
            continue;
        }
    }

    // Собираем zip.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let zip_path = std::env::temp_dir().join(format!("level_{level_number}_{ts}.zip"));
    create_zip(&tmp_dir, &zip_path)?;

    println!(
        "[export] уровень {} → {} ({} ссылок, {} файлов)",
        level_number,
        zip_path.display(),
        refs.len(),
        copied.len()
    );

    Ok(zip_path)
}

/// Импорт .zip в `%APPDATA%/.infinity-backrooms/`.
/// Распаковывает level.xml + assets/ в правильные места.
pub fn import_level_zip(paths: &AppPaths, zip_path: &Path) -> Result<u32> {
    if !zip_path.is_file() {
        return Err(anyhow!("zip не найден: {}", zip_path.display()));
    }

    let tmp_dir = std::env::temp_dir().join("infinity-backrooms-import");
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    fs::create_dir_all(&tmp_dir)?;

    extract_zip(zip_path, &tmp_dir)?;

    // level.xml
    let level_xml = tmp_dir.join("level.xml");
    if !level_xml.is_file() {
        return Err(anyhow!("в архиве нет level.xml"));
    }
    let xml_text = fs::read_to_string(&level_xml)?;
    let level = parse_level_xml(&xml_text)?;
    let number = parse_level_number_from_id(&level.id).unwrap_or(0);

    let target_xml = paths.level_xml(number);
    if let Some(p) = target_xml.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(&target_xml, &xml_text)?;

    // assets/textures/* → cache-files/*
    let tex_src = tmp_dir.join("assets").join("textures");
    if tex_src.is_dir() {
        for entry in fs::read_dir(&tex_src)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let target = paths.cache_files_dir.join(entry.file_name());
                fs::copy(entry.path(), &target)?;
            }
        }
    }

    // assets/models/*_files/ → cache-files/*_files/
    let models_src = tmp_dir.join("assets").join("models");
    if models_src.is_dir() {
        for entry in fs::read_dir(&models_src)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                let target = paths.cache_files_dir.join(&name);
                copy_dir_recursive(&entry.path(), &target)?;
            }
        }
    }

    println!(
        "[import] уровень {} распакован: {}",
        number,
        target_xml.display()
    );
    Ok(number)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Ищет подпапку `*_files` в cache-files/, в которой лежит файл `bare`.
/// Пример: bare = "Wooden_Crate_01_files/Wooden_Crate_01.gltf" → вернёт
/// `cache-files/Wooden_Crate_01_files/`.
fn find_model_dir(cache_dir: &Path, bare: &str) -> Option<PathBuf> {
    // Первый сегмент пути — имя папки.
    let first = bare.split('/').next()?;
    if !first.ends_with("_files") {
        return None;
    }
    let dir = cache_dir.join(first);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &target)?;
        } else {
            fs::copy(&p, &target)?;
        }
    }
    Ok(())
}

/// Парсит номер уровня из id="level_7" или id="l7".
fn parse_level_number_from_id(id: &str) -> Option<u32> {
    let digits: String = id
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

// ---------------------------------------------------------------------------
// Мини-zip: собираем архив вручную (stored, без сжатия).
// ---------------------------------------------------------------------------
//
// Формат:
//   [Local File Header][file data]...[Central Directory][EOCD]
//
// Никакого deflate — просто stored (метод 0). Этого достаточно для
// самодельного экспорта, .zip читается любым архиватором.
// CRC32 считается вручную.

fn create_zip(src_dir: &Path, out_zip: &Path) -> Result<()> {
    let file = fs::File::create(out_zip)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut central: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;

    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(src_dir, &mut files)?;
    files.sort();

    for path in &files {
        let rel = path
            .strip_prefix(src_dir)
            .map_err(|_| anyhow!("path strip failed"))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let data = fs::read(path)?;
        let crc = crc32(&data);
        let size = data.len() as u32;
        let name_bytes = rel_str.as_bytes();
        let name_len = name_bytes.len() as u16;

        // Local File Header.
        writer.write_all(&0x04034b50u32.to_le_bytes())?; // signature
        writer.write_all(&20u16.to_le_bytes())?;         // version needed
        writer.write_all(&0u16.to_le_bytes())?;          // flags
        writer.write_all(&0u16.to_le_bytes())?;          // method = stored
        writer.write_all(&0u16.to_le_bytes())?;          // mod time
        writer.write_all(&0u16.to_le_bytes())?;          // mod date
        writer.write_all(&crc.to_le_bytes())?;           // crc32
        writer.write_all(&size.to_le_bytes())?;          // compressed size
        writer.write_all(&size.to_le_bytes())?;          // uncompressed size
        writer.write_all(&name_len.to_le_bytes())?;      // file name length
        writer.write_all(&0u16.to_le_bytes())?;          // extra field length
        writer.write_all(name_bytes)?;                   // file name
        writer.write_all(&data)?;                        // data

        // Central Directory entry.
        let local_offset = offset;
        offset += 30 + name_len as u32 + size;

        central.write_all(&0x02014b50u32.to_le_bytes())?; // signature
        central.write_all(&20u16.to_le_bytes())?;          // version made by
        central.write_all(&20u16.to_le_bytes())?;          // version needed
        central.write_all(&0u16.to_le_bytes())?;           // flags
        central.write_all(&0u16.to_le_bytes())?;           // method
        central.write_all(&0u16.to_le_bytes())?;           // mod time
        central.write_all(&0u16.to_le_bytes())?;           // mod date
        central.write_all(&crc.to_le_bytes())?;            // crc32
        central.write_all(&size.to_le_bytes())?;           // compressed size
        central.write_all(&size.to_le_bytes())?;           // uncompressed size
        central.write_all(&name_len.to_le_bytes())?;       // file name length
        central.write_all(&0u16.to_le_bytes())?;           // extra length
        central.write_all(&0u16.to_le_bytes())?;           // comment length
        central.write_all(&0u16.to_le_bytes())?;           // disk number
        central.write_all(&0u16.to_le_bytes())?;           // internal attrs
        central.write_all(&0u32.to_le_bytes())?;           // external attrs
        central.write_all(&local_offset.to_le_bytes())?;   // local header offset
        central.write_all(name_bytes)?;                    // file name
    }

    // Central Directory.
    let central_offset = offset;
    let central_size = central.len() as u32;
    writer.write_all(&central)?;

    // EOCD.
    writer.write_all(&0x06054b50u32.to_le_bytes())?;      // signature
    writer.write_all(&0u16.to_le_bytes())?;               // disk number
    writer.write_all(&0u16.to_le_bytes())?;               // disk with central dir
    writer.write_all(&(files.len() as u16).to_le_bytes())?;  // entries on disk
    writer.write_all(&(files.len() as u16).to_le_bytes())?;  // total entries
    writer.write_all(&central_size.to_le_bytes())?;       // central dir size
    writer.write_all(&central_offset.to_le_bytes())?;     // central dir offset
    writer.write_all(&0u16.to_le_bytes())?;               // comment length

    writer.flush()?;
    Ok(())
}

fn extract_zip(zip_path: &Path, out_dir: &Path) -> Result<()> {
    let mut f = fs::File::open(zip_path)?;
    let mut data = Vec::new();
    f.read_to_end(&mut data)?;

    let mut i = 0usize;
    while i + 30 <= data.len() {
        let sig = u32::from_le_bytes([data[i], data[i+1], data[i+2], data[i+3]]);
        if sig != 0x04034b50 {
            // Дошли до central directory / EOCD — выходим.
            break;
        }
        let method = u16::from_le_bytes([data[i+8], data[i+9]]);
        let comp_size = u32::from_le_bytes([data[i+18], data[i+19], data[i+20], data[i+21]]) as usize;
        let name_len = u16::from_le_bytes([data[i+26], data[i+27]]) as usize;
        let extra_len = u16::from_le_bytes([data[i+28], data[i+29]]) as usize;

        let name_start = i + 30;
        let name_end = name_start + name_len;
        if name_end > data.len() { break; }
        let name = std::str::from_utf8(&data[name_start..name_end])
            .map_err(|_| anyhow!("zip: bad name utf-8"))?
            .to_string();

        let data_start = name_end + extra_len;
        let data_end = data_start + comp_size;
        if data_end > data.len() { break; }

        if method != 0 {
            return Err(anyhow!("zip: unsupported compression method {}", method));
        }

        let out_path = out_dir.join(&name);
        if let Some(p) = out_path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&out_path, &data[data_start..data_end])?;

        i = data_end;
    }

    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_files(&p, out)?;
        } else if p.is_file() {
            out.push(p);
        }
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    // Стандартный CRC-32 (IEEE 802.3), полином 0xEDB88320.
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn crc32_known_value() {
        // "123456789" → 0xCBF43926 (стандартный тест-вектор).
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn parse_level_number() {
        assert_eq!(parse_level_number_from_id("level_7"), Some(7));
        assert_eq!(parse_level_number_from_id("l42"), Some(42));
        assert_eq!(parse_level_number_from_id("nothing"), None);
    }

    #[test]
    fn zip_roundtrip() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"hello").unwrap();
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/b.txt"), b"world").unwrap();

        let zip = dir.path().join("test.zip");
        create_zip(&src, &zip).unwrap();
        assert!(zip.is_file());

        let out = dir.path().join("out");
        extract_zip(&zip, &out).unwrap();
        assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(out.join("sub/b.txt")).unwrap(), b"world");
    }

    #[test]
    fn export_import_level_roundtrip() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let paths = crate::paths::AppPaths::from_root(root.clone());
        paths.ensure_dirs().unwrap();

        // Кладём уровень.
        let xml = r#"<level id="level_3" format="bds-level/2" seed="1">
<chunk_size x="32" y="16" z="32"/>
<resources>
<texture id="tex_a" src="assets/textures/Brick_diff.png"/>
</resources>
<chunks><chunk id="c" generator="none" chance="0">
<entity id="player_start" type="player"><transform pos="0 1 0"/></entity>
</chunk></chunks>
</level>"#;
        fs::write(paths.level_xml(3), xml).unwrap();

        // Кладём «ассет».
        fs::write(paths.cache_files_dir.join("Brick_diff.png"), b"\x89PNG\r\n\x1a\n_").unwrap();

        // Экспорт.
        let zip = export_level_zip(&paths, 3).unwrap();
        assert!(zip.is_file());

        // Удаляем всё — потом импортируем.
        fs::remove_file(paths.level_xml(3)).unwrap();
        fs::remove_file(paths.cache_files_dir.join("Brick_diff.png")).unwrap();

        // Импорт.
        let n = import_level_zip(&paths, &zip).unwrap();
        assert_eq!(n, 3);
        assert!(paths.level_xml(3).is_file());
        assert!(paths.cache_files_dir.join("Brick_diff.png").is_file());
    }
}
