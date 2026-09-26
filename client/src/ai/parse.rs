//! Собственный парсер XML BDS Level Format v1/v2 — без сторонних библиотек.
//!
//! Заменяет `roxmltree` полностью. Умеет:
//!   * вырезать `<level>...</level>` из болтовни вокруг;
//!   * срезать markdown-фенсы ```xml ... ```;
//!   * санитайзить Unicode (em-dash, типографские кавычки, NBSP, zero-width);
//!   * **прощать `<`, `>`, `&` внутри `<lua>...</lua>` и других CDATA-подобных блоков**
//!     (главная причина, почему roxmltree падал на Lua-скриптах с `if a < b`);
//!   * проверять парность тегов, закрытие кавычек, наличие имени у тега;
//!   * проверять обязательные секции BDS v2 (chunk_size, chunks, generator, chance).

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct ExtractedXml {
    pub xml: String,
    pub note: &'static str,
    pub sanitized_chars: usize,
}

/// Публичный вход: вернуть только XML-строку.
pub fn extract_level_xml(raw: &str) -> Result<String> {
    Ok(extract_level_xml_diag(raw)?.xml)
}

/// Расширенная версия с диагностикой.
pub fn extract_level_xml_diag(raw: &str) -> Result<ExtractedXml> {
    if raw.trim().is_empty() {
        return Err(anyhow!("ответ ИИ пуст"));
    }

    // 1. Снимаем markdown-фенс.
    let (cleaned, mut note) = strip_markdown_with_note(raw);

    // 2. Ищем <level>...</level>.
    let slice = match find_level_bounds(&cleaned) {
        Some(s) => s,
        None => {
            let preview: String = raw.chars().take(500).collect();
            return Err(anyhow!(
                "в ответе ИИ нет тега <level>...</level>.\n---\n{preview}\n---"
            ));
        }
    };
    if cleaned.len() != slice.len() && note == "clean" {
        note = "found_after_prose";
    }

    // 3. Санитайз Unicode.
    let (unified, sanitized_chars) = sanitize_xml_chars(&slice);
    if sanitized_chars > 0 {
        println!("[parse] sanitize: заменено {} невалидных символов", sanitized_chars);
    }

    // 4. Валидация — своим парсером.
    validate_xml(&unified)?;

    // 5. Проверка обязательных секций v2.
    validate_v2_or_v1(&unified)?;

    Ok(ExtractedXml {
        xml: unified,
        note,
        sanitized_chars,
    })
}

// ===========================================================================
// XML sanitize (Unicode → ASCII)
// ===========================================================================

fn sanitize_xml_chars(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut count = 0usize;

    for ch in s.chars() {
        let replacement: Option<char> = match ch {
            '\u{2014}' | '\u{2013}' | '\u{2012}' | '\u{2011}' => Some('-'),
            '\u{00AB}' | '\u{00BB}' | '\u{201E}' | '\u{201C}' | '\u{201D}' => Some('"'),
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => Some('\''),
            '\u{2026}' => Some('.'),
            '\u{00A0}' | '\u{202F}' | '\u{2009}' | '\u{200A}' | '\u{FEFF}' => Some(' '),
            '\u{200B}' | '\u{200C}' | '\u{200D}' => None,
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => None,
            _ => {
                out.push(ch);
                continue;
            }
        };
        match replacement {
            Some(r) => { out.push(r); count += 1; }
            None => { count += 1; }
        }
    }
    (out, count)
}

// ===========================================================================
// Markdown / границы
// ===========================================================================

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

// ===========================================================================
// Собственный XML-валидатор
// ===========================================================================
//
// Полноценный XML-парсер писать не нужно — нам достаточно проверить:
//   1. Все теги парны (открывающий/закрывающий/self-closing).
//   2. Имя тега непустое и не содержит пробелов/`<`/`>`/`&`.
//   3. Атрибуты имеют вид `name="..."` или `name='...'`.
//   4. Значения атрибутов закрыты кавычками.
//   5. Внутри `<lua>...</lua>` (и `<obj>`, `<svg>`) содержимое — сырой текст,
//      в нём `<`, `>`, `&` разрешены и НЕ ломают валидацию.
//
// Всё остальное (что именно в атрибутах) — не наша забота, это уже парсит
// `level::parse` через собственный обход.

/// Валидатор: возвращает Ok(()) или ошибку с указанием строки:столбца.
pub fn validate_xml(src: &str) -> Result<()> {
    let mut stack: Vec<String> = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    // Имена тегов, содержимое которых нужно читать как RAW (сырой текст).
    // Внутри них `<`, `>`, `&` не считаются разметкой.
    const RAW_TAGS: &[&str] = &["lua", "obj", "svg", "json", "text"];

    while i < bytes.len() {
        let b = bytes[i];

        // Считаем позицию для сообщений об ошибках.
        let advance = |i: &mut usize, line: &mut usize, col: &mut usize| {
            if bytes[*i] == b'\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *i += 1;
        };

        // Открывающий тег `<?xml ... ?>` — пропускаем.
        if b == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'?' {
            // Ищем `?>`.
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'?' && bytes[j + 1] == b'>') {
                j += 1;
            }
            if j + 1 >= bytes.len() {
                return Err(xml_err(line, col, "не закрыт `<?xml ... ?>`"));
            }
            // Продвинуть i/line/col до j+2.
            while i < j + 2 {
                advance(&mut i, &mut line, &mut col);
            }
            continue;
        }

        if b == b'<' {
            // Открывающий или закрывающий тег.
            let tag_start_line = line;
            let tag_start_col = col;

            // Следующий символ: '/' → closing, иначе opening.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() {
                return Err(xml_err(tag_start_line, tag_start_col, "незакрытый `<`"));
            }

            let is_closing = bytes[j] == b'/';
            if is_closing {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }

            // Имя тега.
            let name_start = j;
            while j < bytes.len() && is_name_char(bytes[j]) {
                j += 1;
            }
            if j == name_start {
                return Err(xml_err(
                    tag_start_line,
                    tag_start_col,
                    "пустое имя тега после `<`",
                ));
            }
            let name = std::str::from_utf8(&bytes[name_start..j])
                .unwrap_or("")
                .to_string();

            if is_closing {
                // Дочитываем до `>`, там допустимы только пробелы.
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() || bytes[j] != b'>' {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!("закрывающий тег `</{name}` без `>`"),
                    ));
                }
                // Продвинуть до после `>`.
                while i <= j {
                    advance(&mut i, &mut line, &mut col);
                }

                // Проверяем парность.
                match stack.pop() {
                    Some(top) if top == name => {}
                    Some(top) => {
                        return Err(xml_err(
                            tag_start_line,
                            tag_start_col,
                            &format!(
                                "закрывающий тег `</{name}>` не совпадает с `<{top}>`"
                            ),
                        ));
                    }
                    None => {
                        return Err(xml_err(
                            tag_start_line,
                            tag_start_col,
                            &format!("закрывающий тег `</{name}>` без открывающего"),
                        ));
                    }
                }
                continue;
            }

            // Открывающий тег: читаем атрибуты до `>` или `/>`.
            let mut self_closing = false;
            loop {
                // Пропускаем whitespace.
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!("незакрытый тег `<{name}`"),
                    ));
                }
                if bytes[j] == b'>' {
                    break;
                }
                if bytes[j] == b'/' {
                    if j + 1 < bytes.len() && bytes[j + 1] == b'>' {
                        self_closing = true;
                        j += 1;
                        break;
                    } else {
                        return Err(xml_err(
                            tag_start_line,
                            tag_start_col,
                            "`/` не перед `>`",
                        ));
                    }
                }

                // Имя атрибута.
                let attr_start = j;
                while j < bytes.len() && is_attr_name_char(bytes[j]) {
                    j += 1;
                }
                if j == attr_start {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!(
                            "невалидный символ в атрибутах тега <{name}>: `{}`",
                            bytes[j] as char
                        ),
                    ));
                }
                let _attr_name = &bytes[attr_start..j];

                // Пробелы перед `=`.
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() || bytes[j] != b'=' {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!("атрибут без `=` в <{name}>"),
                    ));
                }
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(xml_err(tag_start_line, tag_start_col, "ожидалась `\"` или `'`"));
                }
                let quote = bytes[j];
                if quote != b'"' && quote != b'\'' {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!(
                            "значение атрибута должно быть в кавычках, а не `{}`",
                            quote as char
                        ),
                    ));
                }
                j += 1;
                // Значение атрибута: до такой же кавычки.
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!("не закрыта кавычка в атрибуте <{name}>"),
                    ));
                }
                j += 1; // перешагнуть закрывающую кавычку
            }

            // Продвинуть i/line/col до после `>`.
            while i <= j {
                advance(&mut i, &mut line, &mut col);
            }

            if self_closing {
                continue;
            }

            // Если это RAW-тег — читаем содержимое как сырой текст до `</tag>`.
            if RAW_TAGS.contains(&name.as_str()) {
                let close = format!("</{name}>");
                let rest = &src[i..];
                let pos = rest.find(&close).ok_or_else(|| {
                    xml_err(
                        tag_start_line,
                        tag_start_col,
                        &format!("не найден закрывающий `{close}`"),
                    )
                })?;
                // Прогоняем pos символов вперёд (с обновлением line/col).
                let mut k = 0usize;
                while k < pos {
                    advance(&mut i, &mut line, &mut col);
                    k += 1;
                }
                // Теперь i стоит на '<' закрывающего тега.
                // Дочитываем `</tag>`.
                let close_len = close.len();
                let mut k = 0usize;
                while k < close_len {
                    advance(&mut i, &mut line, &mut col);
                    k += 1;
                }
                continue;
            }

            // Иначе — обычный контейнер: кладём в стек.
            stack.push(name);
            continue;
        }

        // Обычный текст — просто идём дальше. `&` без `;` и прочие
        // невалидности в тексте НЕ проверяем — это ответственность
        // потребителя. Нам важно только чтобы теги были парны.
        advance(&mut i, &mut line, &mut col);
    }

    if !stack.is_empty() {
        return Err(anyhow!(
            "не закрыты теги: {}",
            stack.join(", ")
        ));
    }
    Ok(())
}

fn is_name_char(b: u8) -> bool {
    // Начало/продолжение имени тега: буквы, цифры, _, :, -, .
    b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'-' || b == b'.'
}

fn is_attr_name_char(b: u8) -> bool {
    // Имя атрибута может содержать те же символы.
    is_name_char(b)
}

fn xml_err(line: usize, col: usize, msg: &str) -> anyhow::Error {
    anyhow!("XML: {msg} (строка {line}, столбец {col})")
}

// ===========================================================================
// Проверка BDS Level Format v2
// ===========================================================================

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
    fn ignores_trailing_prose() {
        let raw = format!("blah blah\n{SIMPLE}\nnoise after");
        let got = extract_level_xml(&raw).unwrap();
        assert_eq!(got, SIMPLE);
    }

    #[test]
    fn accepts_v2_with_chunk_size_and_chunk() {
        let v2 = r#"<level id="l2" format="bds-level/2" seed="1"><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c1" generator="default" chance="90"></chunk></chunks></level>"#;
        assert!(extract_level_xml(v2).is_ok());
    }

    #[test]
    fn sanitizes_em_dash() {
        let raw = r#"<level format="bds-level/1"><meta name="Уровень — Тест"/><entities></entities><bounds min="0 0 0" max="1 1 1"/></level>"#;
        let r = extract_level_xml_diag(raw).unwrap();
        assert!(!r.xml.contains('—'));
        assert!(r.sanitized_chars >= 1);
    }

    #[test]
    fn unmatched_close_tag_fails() {
        let bad = r#"<level><entities></foo></entities></level>"#;
        assert!(validate_xml(bad).is_err());
    }

    #[test]
    fn unclosed_open_tag_fails() {
        let bad = r#"<level><entities></level>"#;
        assert!(validate_xml(bad).is_err());
    }

    #[test]
    fn self_closing_ok() {
        let ok = r#"<level><chunk_size x="32" y="16" z="32"/><chunks><chunk id="c" generator="none" chance="0"/></chunks></level>"#;
        assert!(validate_xml(ok).is_ok());
    }

    #[test]
    fn attr_without_quotes_fails() {
        let bad = r#"<level id=foo></level>"#;
        assert!(validate_xml(bad).is_err());
    }

    // === ГЛАВНЫЙ ТЕСТ: `<` внутри <lua> НЕ должен ломать валидатор ===

    #[test]
    fn lua_with_less_than_is_ok() {
        let xml = r#"<level format="bds-level/1"><scripts><script id="s"><lua>if a < b then return end</lua></script></scripts><bounds min="0 0 0" max="1 1 1"/></level>"#;
        assert!(validate_xml(xml).is_ok(), "lua с '<' должен проходить");
        assert!(extract_level_xml(xml).is_ok());
    }

    #[test]
    fn lua_with_ampersand_is_ok() {
        let xml = r#"<level format="bds-level/1"><scripts><script id="s"><lua>local x = a & b</lua></script></scripts><bounds min="0 0 0" max="1 1 1"/></level>"#;
        assert!(validate_xml(xml).is_ok());
    }

    #[test]
    fn lua_with_greater_than_is_ok() {
        let xml = r#"<level format="bds-level/1"><scripts><script id="s"><lua>if a > b then end</lua></script></scripts><bounds min="0 0 0" max="1 1 1"/></level>"#;
        assert!(validate_xml(xml).is_ok());
    }

    #[test]
    fn obj_with_lt_is_ok() {
        let xml = r#"<level format="bds-level/1"><resources><model id="m"><obj>v -1 < 1</obj></model></resources><bounds min="0 0 0" max="1 1 1"/></level>"#;
        assert!(validate_xml(xml).is_ok());
    }

    #[test]
    fn real_level_with_lua_passes() {
        // Приблизительно то, что прислал ИИ.
        let xml = r#"<level format="bds-level/2" seed="7007" spawn_point="player_start">
<meta name="Level EN-7 - Thalassophobia"/>
<chunk_size x="32" y="16" z="32"/>
<chunks>
<chunk id="chunk_spawn" generator="none" chance="0">
<entity id="player_start" type="player">
<transform pos="0 1 0" rot="0 0 0" scale="1 1 1"/>
</entity>
</chunk>
</chunks>
<scripts>
<script id="flicker">
<lua>
local t = 0
local function flicker(dt)
  t = t + dt
  if t < 0.08 then return end
  if math.random() < 0.08 then
    t = 0
  end
end
</lua>
</script>
</scripts>
</level>"#;
        assert!(validate_xml(xml).is_ok(), "реальный XML с Lua-скриптом должен проходить");
        assert!(extract_level_xml(xml).is_ok());
    }
}
