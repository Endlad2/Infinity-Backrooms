//! Парсинг ответа ИИ: срезание markdown, извлечение <level>...</level>, валидация XML (v1 и v2).
//! См. §8.4 ТЗ: «ИИ должен вернуть только валидный XML. Markdown-обёртка срезается.
//! Если XML невалиден — повторный запрос или fallback».

use anyhow::{anyhow, Result};

/// Результат: очищенная XML-строка уровня.
pub fn extract_level_xml(raw: &str) -> Result<String> {
    let cleaned = strip_markdown(raw);
    let slice = find_level_bounds(&cleaned)
        .ok_or_else(|| anyhow!("в ответе ИИ нет тега <level>...</level>"))?;
    validate_xml(slice)?;
    validate_v2_or_v1(slice)?;
    Ok(slice.to_string())
}

/// Проверка структуры BDS Level Format v2.
/// Для format="bds-level/2" требуем наличие <chunk_size> и хотя бы одного
/// <chunk ... generator="..." chance="...">. Для v1-уровней допускается
/// отсутствие этих секций (обратная совместимость).
pub fn validate_v2_or_v1(s: &str) -> Result<()> {
    let is_v2 = s.contains("bds-level/2");
    if !is_v2 {
        return Ok(());
    }
    if !s.contains("<chunk_size") {
        return Err(anyhow!(
            "BDS Level Format v2: отсутствует обязательный <chunk_size x y z/>"
        ));
    }
    if !s.contains("<chunk ") && !s.contains("<chunk\n") && !s.contains("<chunk\r") {
        return Err(anyhow!(
            "BDS Level Format v2: отсутствуют чанки <chunks><chunk .../></chunks>"
        ));
    }
    if !s.contains("generator=") {
        return Err(anyhow!(
            "BDS Level Format v2: у <chunk> нет обязательного атрибута generator=\"default|axis|none\""
        ));
    }
    if !s.contains("chance=") {
        return Err(anyhow!(
            "BDS Level Format v2: у <chunk> нет обязательного атрибута chance=\"0..100\""
        ));
    }
    Ok(())
}

/// Снимаем ```xml ... ``` или ``` ... ```.
pub fn strip_markdown(s: &str) -> String {
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

/// Найти срез от первого `<level` до соответствующего `</level>`.
/// Учитываем атрибуты и вложенность (у level нет вложенных level,
/// но на всякий случай считаем счётчик).
pub fn find_level_bounds(s: &str) -> Option<&str> {
    let start = s.find("<level")?;
    // найдём ">": конец открывающего тега
    let open_end_rel = s[start..].find('>')?;
    let open_end = start + open_end_rel + 1;

    // Ищем первый </level> после открывающего тега
    let close = s[open_end..].find("</level>")?;
    let end = open_end + close + "</level>".len();
    Some(&s[start..end])
}

/// Валидация через roxmltree.
pub fn validate_xml(s: &str) -> Result<()> {
    roxmltree::Document::parse(s)
        .map(|_| ())
        .map_err(|e| anyhow!("XML невалиден: {e}"))
}

/// Скорее всего это ответ с уровнем (эвристика для выбора fallback-модели).
pub fn looks_like_level(s: &str) -> bool {
    let c = strip_markdown(s);
    c.contains("<level") && c.contains("</level>")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = r#"<level id="l1" format="bds-level/1" engine="bevy"><entities></entities><bounds min="0 0 0" max="1 1 1"/></level>"#;

    #[test]
    fn extracts_plain_level() {
        let got = extract_level_xml(SIMPLE).unwrap();
        assert!(got.starts_with("<level"));
        assert!(got.ends_with("</level>"));
    }

    #[test]
    fn strips_markdown_xml_fence() {
        let raw = format!("```xml\n{SIMPLE}\n```");
        let got = extract_level_xml(&raw).unwrap();
        assert!(got.starts_with("<level"));
    }

    #[test]
    fn strips_bare_fence() {
        let raw = format!("```\n{SIMPLE}\n```");
        let got = extract_level_xml(&raw).unwrap();
        assert!(got.starts_with("<level"));
    }

    #[test]
    fn rejects_answer_without_level() {
        assert!(extract_level_xml("no xml here").is_err());
    }

    #[test]
    fn rejects_invalid_xml_inside_level() {
        let bad = "<level><unclosed></level>";
        assert!(extract_level_xml(bad).is_err());
    }

    #[test]
    fn looks_like_detects_level() {
        assert!(looks_like_level(SIMPLE));
        assert!(!looks_like_level("just text"));
    }

    #[test]
    fn ignores_trailing_prose() {
        let raw = format!("blah blah\n{SIMPLE}\nnoise after");
        let got = extract_level_xml(&raw).unwrap();
        assert_eq!(got, SIMPLE);
    }

    #[test]
    fn accepts_v2_with_chunk_size_and_chunk() {
        let v2 = r#"<level id="l2" format="bds-level/2" seed="1"><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c1" generator="default" chance="90"></chunk></chunks></level>"#;
        let got = extract_level_xml(v2).unwrap();
        assert!(got.contains("chunk_size"));
    }

    #[test]
    fn rejects_v2_without_chunk_size() {
        let bad = r#"<level id="l2" format="bds-level/2"><chunks><chunk id="c1" generator="default" chance="90"></chunk></chunks></level>"#;
        assert!(extract_level_xml(bad).is_err());
    }

    #[test]
    fn rejects_v2_without_chunk() {
        let bad = r#"<level id="l2" format="bds-level/2"><chunk_size x="32" y="16" z="32"/><chunks></chunks></level>"#;
        assert!(extract_level_xml(bad).is_err());
    }

    #[test]
    fn rejects_v2_chunk_without_generator() {
        let bad = r#"<level id="l2" format="bds-level/2"><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c1" chance="90"></chunk></chunks></level>"#;
        assert!(extract_level_xml(bad).is_err());
    }

    #[test]
    fn accepts_v1_without_chunks() {
        // v1-уровни не требуют chunk_size/chunks
        assert!(extract_level_xml(SIMPLE).is_ok());
    }
}