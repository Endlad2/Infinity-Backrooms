//! ИИ-генерация ассетов через локальный OpenAI-совместимый прокси
//! http://localhost:9655 (модель deepseek-chat).
//!
//! Генерирует:
//!   * SVG-текстуру по тегам -> растрирует в PNG (см. assets::svg).
//!   * OBJ-модель (текстом) по тегам.
//!
//! При недоступности бэкенда — fallback: однотонная SVG-заглушка / минимальный OBJ.

use anyhow::{anyhow, Result};
use serde_json::json;

use super::svg;

pub const AI_ENDPOINT: &str = "http://localhost:9655/v1/chat/completions";
pub const AI_MODEL_ASSETS: &str = "deepseek-chat";

/// Сгенерировать SVG-текстуру по тегам. Возвращает SVG-строку.
pub fn generate_svg(prompt: &str, tags: &[String]) -> Result<String> {
    let sys = "Ты генератор SVG-текстур. Отвечай ТОЛЬКО валидным SVG-кодом, без пояснений и markdown.";
    let user = format!(
        "Сгенерируй бесшовную текстуру по тегам: {}\nДополнительное описание: {}\n\
         Размер viewBox: 256x256. Верни только SVG.",
        tags.join(", "),
        prompt
    );
    match call_chat(sys, &user) {
        Ok(text) => Ok(strip_code_fence(&text).trim().to_string()),
        Err(_) => {
            // fallback — цветная заглушка по первому тегу
            let color = tag_color(tags.first().map(|s| s.as_str()).unwrap_or("grey"));
            Ok(svg::solid_color_svg(color, 256, 256))
        }
    }
}

/// Растеризовать SVG-текстуру в PNG.
pub fn generate_texture_png(prompt: &str, tags: &[String], size: u32) -> Result<Vec<u8>> {
    let svg_str = generate_svg(prompt, tags)?;
    let (w, h) = if size == 0 { (256, 256) } else { (size, size) };
    svg::svg_to_png(&svg_str, w, h)
}

/// Сгенерировать OBJ-модель по тегам. Возвращает OBJ-текст.
pub fn generate_obj(prompt: &str, tags: &[String]) -> Result<String> {
    let sys = "Ты генератор OBJ-моделей. Отвечай ТОЛЬКО валидным Wavefront OBJ, без пояснений и markdown.";
    let user = format!(
        "Сгенерируй low-poly OBJ-модель по тегам: {}\nДополнительное описание: {}\n\
         Верни только OBJ. Минимум: заголовок `o model`, вершины `v`, грани `f`.",
        tags.join(", "),
        prompt
    );
    match call_chat(sys, &user) {
        Ok(text) => Ok(strip_code_fence(&text).trim().to_string()),
        Err(_) => Ok(fallback_cube_obj()),
    }
}

fn call_chat(system: &str, user: &str) -> Result<String> {
    let body = json!({
        "model": AI_MODEL_ASSETS,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "stream": false,
    });
    let resp = ureq::post(AI_ENDPOINT)
        .timeout(std::time::Duration::from_secs(60))
        .send_json(body)
        .map_err(|e| anyhow!("AI backend error: {e}"))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| anyhow!("AI json parse error: {e}"))?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("AI ответ без content"))?
        .to_string();
    Ok(content)
}

/// Убрать markdown-обёртку ```...``` и ```svg/obj ...```.
pub fn strip_code_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // первая строка может содержать язык
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

fn tag_color(tag: &str) -> &'static str {
    match tag {
        "yellow" => "#d8c46a",
        "grey" | "gray" => "#7a7a7a",
        "red" => "#a03028",
        "green" => "#3f7a3f",
        "blue" => "#2f4f7a",
        "brown" | "wood" => "#8a6a3a",
        "black" => "#151515",
        "white" => "#efefef",
        _ => "#606060",
    }
}

fn fallback_cube_obj() -> String {
    "\
o cube
v -0.5 -0.5 -0.5
v  0.5 -0.5 -0.5
v  0.5  0.5 -0.5
v -0.5  0.5 -0.5
v -0.5 -0.5  0.5
v  0.5 -0.5  0.5
v  0.5  0.5  0.5
v -0.5  0.5  0.5
f 1 2 3 4
f 5 8 7 6
f 1 5 6 2
f 2 6 7 3
f 3 7 8 4
f 4 8 5 1
"
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_fence_plain() {
        assert_eq!(strip_code_fence("hello"), "hello");
    }

    #[test]
    fn strip_fence_svg() {
        let s = "```svg\n<svg/>\n```";
        assert_eq!(strip_code_fence(s), "<svg/>");
    }

    #[test]
    fn strip_fence_no_lang() {
        let s = "```\nline\n```";
        assert_eq!(strip_code_fence(s), "line");
    }

    #[test]
    fn fallback_obj_is_valid_looking() {
        let obj = fallback_cube_obj();
        assert!(obj.contains("o cube"));
        assert!(obj.contains("f 1 2 3 4"));
    }

    #[test]
    fn texture_falls_back_when_backend_missing() {
        // бэкенд почти наверняка недоступен в тестовой среде — ожидаем fallback-заглушку
        let png = generate_texture_png("", &["yellow".into(), "wall".into()], 32);
        assert!(png.is_ok());
        let bytes = png.unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}