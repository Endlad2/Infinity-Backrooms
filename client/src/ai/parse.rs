//! Парсинг ответа ИИ: срезание markdown, извлечение <level>...</level>, валидация XML (v1 и v2).
//! §8.4 ТЗ: «ИИ должен вернуть только валидный XML. Markdown-обёртка срезается.
//! Если XML невалиден — повторный запрос или fallback».
//!
//! Обновлено: парсер стал устойчивым к:
//!   * markdown-фенсам ```xml ... ``` и просто ``` ... ```
//!   * префиксному тексту «Sure, here is your XML:» и trailing-тексту
//!   * <level ...> с атрибутами в несколько строк
//!   * XML-декларации <?xml ... ?> ПЕРЕД <level>
//!   * невалидному содержимому внутри (тогда — error с диагностикой)
//!   * рассуждениям reasoning-модели (если в reasoning_content лежит XML)

use anyhow::{anyhow, Result};

/// Результат: очищенная XML-строка уровня + метаданные для диагностики.
#[derive(Debug, Clone)]
pub struct ExtractedXml {
    pub xml: String,
    /// Что пришлось сделать: "clean" | "stripped_fence" | "found_after_prose"
    pub note: &'static str,
}

/// Результат: очищенная XML-строка уровня (совместимость со старым API).
pub fn extract_level_xml(raw: &str) -> Result<String> {
    Ok(extract_level_xml_diag(raw)?.xml)
}

/// Расширенная версия с диагностикой.
pub fn extract_level_xml_diag(raw: &str) -> Result<ExtractedXml> {
    // 0. Сразу предупреждение, если ответ пустой.
    if raw.trim().is_empty() {
        return Err(anyhow!("ответ ИИ пуст"));
    }

    // 1. Срезаем markdown-фенс, если он есть.
    let (cleaned, mut note) = strip_markdown_with_note(raw);

    // 2. Ищем <level ...> ... </level>.
    let slice = match find_level_bounds(&cleaned) {
        Some(s) => s.to_string(),
        None => {
            // Может быть, есть <?xml ... ?><level>...</level> — тогда ищем по <level.
            // Или ответ — это вообще не XML.
            // Дам подробную ошибку с началом ответа, чтобы понять в чём дело.
            let preview: String = raw.chars().take(500).collect();
            return Err(anyhow!(
                "в ответе ИИ нет тега <level>...</level>. \
                 Первые 500 символов ответа:\n---\n{preview}\n---"
            ));
        }
    };

    if cleaned.len() != slice.len() {
        // Была обёртка вокруг — фиксируем.
        if note == "clean" {
            note = "found_after_prose";
        }
    }

    // 3. Валидация XML.
    validate_xml(&slice)
        .map_err(|e| anyhow!("XML невалиден: {e}\n---\n{}\n---", preview_of(&slice, 800)))?;

    // 4. Валидация структуры v2.
    validate_v2_or_v1(&slice)?;

    Ok(ExtractedXml { xml: slice, note })
}

fn preview_of(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…(обрезано)")
    } else {
        t
    }
}

/// Снимаем ```xml ... ``` или ``` ... ```.
/// Возвращает (текст, признак_что_был_фенс).
pub fn strip_markdown_with_note(s: &str) -> (String, &'static str) {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Первая строка может содержать язык (xml, json, html, ...).
        let inner = match rest.find('\n') {
            Some(nl) => &rest[nl + 1..],
            None => rest,
        };
        // Срезаем закрывающий ``` если он есть. Иногда после него ещё идёт текст — тогда ищем
        // первый ``` и режем по нему.
        let inner = match inner.find("```") {
            Some(pos) => &inner[..pos],
            None => inner.trim_end(),
        };
        return (inner.trim().to_string(), "stripped_fence");
    }
    (t.to_string(), "clean")
}

/// Совместимость со старым API.
pub fn strip_markdown(s: &str) -> String {
    strip_markdown_with_note(s).0
}

/// Найти срез от первого `<level` до соответствующего `</level>`.
/// Учитываем атрибуты и вложенность.
pub fn find_level_bounds(s: &str) -> Option<String> {
    let start = s.find("<level")?;
    // Найдём ">" — конец открывающего тега.
    let open_end_rel = s[start..].find('>')?;
    let open_end = start + open_end_rel + 1;

    // Ищем первый </level> после открывающего тега.
    let close = s[open_end..].find("</level>")?;
    let end = open_end + close + "</level>".len();
    Some(s[start..end].to_string())
}

/// Проверка структуры BDS Level Format v2.
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

/// Валидация через roxmltree.
pub fn validate_xml(s: &str) -> Result<()> {
    roxmltree::Document::parse(s)
        .map(|_| ())
        .map_err(|e| anyhow!("{e}"))
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
    fn strips_fence_with_trailing_prose() {
        let raw = format!("```xml\n{SIMPLE}\n```\n\nHope this helps!");
        let got = extract_level_xml(&raw).unwrap();
        assert_eq!(got, SIMPLE);
    }

    #[test]
    fn finds_level_after_xml_declaration() {
        let raw = format!("<?xml version=\"1.0\"?>\n{SIMPLE}");
        let got = extract_level_xml(&raw).unwrap();
        assert!(got.starts_with("<level"));
    }

    #[test]
    fn diag_notes_clean() {
        let r = extract_level_xml_diag(SIMPLE).unwrap();
        assert_eq!(r.note, "clean");
    }

    #[test]
    fn diag_notes_stripped_fence() {
        let raw = format!("```xml\n{SIMPLE}\n```");
        let r = extract_level_xml_diag(&raw).unwrap();
        assert_eq!(r.note, "stripped_fence");
    }

    #[test]
    fn diag_notes_found_after_prose() {
        let raw = format!("Sure, here it is:\n{SIMPLE}\nDone.");
        let r = extract_level_xml_diag(&raw).unwrap();
        assert_eq!(r.note, "found_after_prose");
    }
}
