//! Парсинг ответа ИИ: срезание markdown, извлечение <level>...</level>,
//! sanitize невалидных для XML символов, валидация XML (v1 и v2).
//!
//! §8.4 ТЗ: «ИИ должен вернуть только валидный XML. Markdown-обёртка срезается.
//! Если XML невалиден — повторный запрос или fallback».
//!
//! Обновлено: теперь парсер умеет:
//!   * срезать markdown-фенсы ```xml ... ``` и ``` ... ```
//!   * находить <level>...</level> среди болтовни
//!   * заменять невалидные для XML Unicode-символы (em-dash —, en-dash –,
//!     типографские кавычки «»„", …, NBSP) на ASCII-эквиваленты
//!   * вырезать управляющие символы (кроме \t \n \r)

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct ExtractedXml {
    pub xml: String,
    /// Что пришлось сделать: "clean" | "stripped_fence" | "found_after_prose"
    pub note: &'static str,
    /// Сколько символов заменено санитайзером.
    pub sanitized_chars: usize,
}

/// Результат: очищенная XML-строка уровня (совместимость со старым API).
pub fn extract_level_xml(raw: &str) -> Result<String> {
    Ok(extract_level_xml_diag(raw)?.xml)
}

/// Расширенная версия с диагностикой.
pub fn extract_level_xml_diag(raw: &str) -> Result<ExtractedXml> {
    if raw.trim().is_empty() {
        return Err(anyhow!("ответ ИИ пуст"));
    }

    // 1. Срезаем markdown-фенс, если он есть.
    let (cleaned, mut note) = strip_markdown_with_note(raw);

    // 2. Ищем <level ...> ... </level>.
    let slice = match find_level_bounds(&cleaned) {
        Some(s) => s,
        None => {
            let preview: String = raw.chars().take(500).collect();
            return Err(anyhow!(
                "в ответе ИИ нет тега <level>...</level>. \
                 Первые 500 символов ответа:\n---\n{preview}\n---"
            ));
        }
    };

    if cleaned.len() != slice.len() && note == "clean" {
        note = "found_after_prose";
    }

    // 3. Санитайз невалидных для XML символов.
    let (sanitized, sanitized_chars) = sanitize_xml_chars(&slice);
    if sanitized_chars > 0 {
        println!(
            "[parse] санитайз: заменено {} невалидных для XML символов",
            sanitized_chars
        );
    }

    // 4. Валидация XML (уже после санитайза).
    match validate_xml(&sanitized) {
        Ok(()) => {}
        Err(e) => {
            // Попробуем один раз без санитайза — вдруг sanitize что-то сломал.
            if sanitized_chars > 0 && validate_xml(&slice).is_ok() {
                return Ok(ExtractedXml {
                    xml: slice,
                    note,
                    sanitized_chars: 0,
                });
            }
            return Err(anyhow!(
                "XML невалиден: {e}\n---\n{}\n---",
                preview_of(&sanitized, 800)
            ));
        }
    }

    // 5. Валидация структуры v2.
    validate_v2_or_v1(&sanitized)?;

    Ok(ExtractedXml {
        xml: sanitized,
        note,
        sanitized_chars,
    })
}

/// Заменяет невалидные для XML Unicode-символы на ASCII.
/// Возвращает (новый_текст, сколько_символов_заменено).
///
/// Правила:
///   * em-dash (U+2014), en-dash (U+2013), figure-dash (U+2012) → `-`
///   * типографские кавычки « » „ ‟ " " ' ' ‚ ‛ → обычные `"` или `'`
///   * многоточие … (U+2026) → `...`
///   * NBSP (U+00A0), узкий NBSP (U+202F) → обычный пробел
///   * неразрывный дефис (U+2011) → `-`
///   * любой control-символ кроме \t (0x09), \n (0x0A), \r (0x0D) → удаляется
fn sanitize_xml_chars(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut count = 0usize;

    for ch in s.chars() {
        let replacement: Option<char> = match ch {
            '\u{2014}' => Some('-'),  // em-dash
            '\u{2013}' => Some('-'),  // en-dash
            '\u{2012}' => Some('-'),  // figure-dash
            '\u{2011}' => Some('-'),  // non-breaking hyphen
            '\u{00AB}' => Some('"'),  // «
            '\u{00BB}' => Some('"'),  // »
            '\u{201E}' => Some('"'),  // „
            '\u{201C}' => Some('"'),  // "
            '\u{201D}' => Some('"'),  // "
            '\u{2018}' => Some('\''), // '
            '\u{2019}' => Some('\''), // '
            '\u{201A}' => Some('\''), // ‚
            '\u{201B}' => Some('\''), // ‛
            '\u{2026}' => Some('.'),  // … → один ".", но обычно идёт "..."
            '\u{00A0}' => Some(' '),  // NBSP
            '\u{202F}' => Some(' '),  // narrow NBSP
            '\u{2009}' => Some(' '),  // thin space
            '\u{200A}' => Some(' '),  // hair space
            '\u{FEFF}' => Some(' '),  // BOM / zero-width no-break
            '\u{200B}' => None,       // zero-width space — просто выкидываем
            '\u{200C}' => None,       // zero-width non-joiner — выкидываем
            '\u{200D}' => None,       // zero-width joiner — выкидываем
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {
                // Управляющий символ — выкидываем.
                None
            }
            _ => {
                out.push(ch);
                continue;
            }
        };

        match replacement {
            Some(r) => {
                out.push(r);
                count += 1;
            }
            None => {
                // Символ убираем — тоже считаем как изменение.
                count += 1;
            }
        }
    }

    // NBSP и пр. съедаются и без счётчика: если out.len() != s.len() — уже
    // заметили выше. Но если длина совпала случайно — не важно.
    (out, count)
}

fn preview_of(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…(обрезано)")
    } else {
        t
    }
}

pub fn strip_markdown_with_note(s: &str) -> (String, &'static str) {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let inner = match rest.find('\n') {
            Some(nl) => &rest[nl + 1..],
            None => rest,
        };
        let inner = match inner.find("```") {
            Some(pos) => &inner[..pos],
            None => inner.trim_end(),
        };
        return (inner.trim().to_string(), "stripped_fence");
    }
    (t.to_string(), "clean")
}

pub fn strip_markdown(s: &str) -> String {
    strip_markdown_with_note(s).0
}

pub fn find_level_bounds(s: &str) -> Option<String> {
    let start = s.find("<level")?;
    let open_end_rel = s[start..].find('>')?;
    let open_end = start + open_end_rel + 1;
    let close = s[open_end..].find("</level>")?;
    let end = open_end + close + "</level>".len();
    Some(s[start..end].to_string())
}

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
            "BDS Level Format v2: у <chunk> нет обязательного атрибута generator"
        ));
    }
    if !s.contains("chance=") {
        return Err(anyhow!(
            "BDS Level Format v2: у <chunk> нет обязательного атрибута chance"
        ));
    }
    Ok(())
}

pub fn validate_xml(s: &str) -> Result<()> {
    roxmltree::Document::parse(s)
        .map(|_| ())
        .map_err(|e| anyhow!("{e}"))
}

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
    fn rejects_answer_without_level() {
        assert!(extract_level_xml("no xml here").is_err());
    }

    #[test]
    fn rejects_invalid_xml_inside_level() {
        let bad = "<level><unclosed></level>";
        assert!(extract_level_xml(bad).is_err());
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
    fn diag_notes_clean() {
        let r = extract_level_xml_diag(SIMPLE).unwrap();
        assert_eq!(r.note, "clean");
        assert_eq!(r.sanitized_chars, 0);
    }

    // ---- ключевые новые тесты ----

    #[test]
    fn sanitizes_em_dash_in_attribute() {
        // Именно это падало у пользователя: <meta name="Уровень 7 — Талассофобия">
        let raw = r#"<level format="bds-level/2" seed="1"><meta name="Уровень 7 — Талассофобия"/><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c" generator="none" chance="0"></chunk></chunks></level>"#;
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('—'), "em-dash должен быть заменён");
        assert!(r.xml.contains("Уровень 7 - Талассофобия"));
        assert!(r.sanitized_chars >= 1);
    }

    #[test]
    fn sanitizes_en_dash_and_ellipsis() {
        let raw = r#"<level format="bds-level/1"><entities id="a–b…c"></entities><bounds min="0 0 0" max="1 1 1"/></level>"#;
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('–'));
        assert!(!r.xml.contains('…'));
    }

    #[test]
    fn sanitizes_typographic_quotes() {
        let raw = "<level format=\"bds-level/1\"><entities name=\"a «b» c\"></entities><bounds min=\"0 0 0\" max=\"1 1 1\"/></level>";
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('«'));
        assert!(!r.xml.contains('»'));
    }

    #[test]
    fn sanitizes_nbsp() {
        let raw = "<level format=\"bds-level/1\"><entities id=\"a\u{00A0}b\"></entities><bounds min=\"0 0 0\" max=\"1 1 1\"/></level>";
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('\u{00A0}'));
    }

    #[test]
    fn drops_zero_width_space() {
        let raw = "<level format=\"bds-level/1\"><entities id=\"a\u{200B}b\"></entities><bounds min=\"0 0 0\" max=\"1 1 1\"/></level>";
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('\u{200B}'));
    }

    #[test]
    fn valid_xml_untouched() {
        let raw = r#"<level format="bds-level/1"><entities id="plain"></entities><bounds min="0 0 0" max="1 1 1"/></level>"#;
        let r = extract_level_xml_diag(raw).unwrap();
        assert_eq!(r.sanitized_chars, 0);
        assert_eq!(r.xml, raw);
    }
}
