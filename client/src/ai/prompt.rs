//! Сборка промта для генерации уровня в BDS Level Format v2 XML (chunked).
//! Источники (§6.3, §6.4 ТЗ):
//!   1. Системная инструкция: сгенерировать BDS-level/2 XML.
//!   2. HTML вики-страницы: https://www.backroomswiki.ru/level-{N}
//!   3. Список файлов из cache-files/ (Этап 1, Poly Haven) — что уже скачано.
//!   4. (Опционально) замечания пользователя от прошлой генерации.
//!   5. Требование: только валидный XML, все Lua-скрипты инлайн.

use crate::ai::client::{ChatRequest, MODEL_LEVEL_SEARCH, MODEL_REASONER};

pub const WIKI_BASE: &str = "https://www.backroomswiki.ru/level-";

/// Системная инструкция — должна стабильно давать XML по формату bds-level/2.
pub const SYSTEM_INSTRUCTION: &str = "\
Ты генератор уровней для игры Backrooms Infinity. Формат вывода — BDS Level Format v2 (XML, бесконечные чанковые уровни). \
Строгие требования:\n\
- Верни ТОЛЬКО валидный XML, начинающийся с <level ...> и заканчивающийся </level>. \
Никакого текста до/после, без markdown-обёрток.\n\
- Атрибут format=\"bds-level/2\", engine=\"bevy\", version=\"2.0\". ОБЯЗАТЕЛЬНО задай seed=\"<целое>\".\n\
- Уровень НЕ хранит сущности напрямую: ВСЁ живёт внутри <chunks><chunk>...</chunk></chunks>. \
Top-level <entities> не используй.\n\
- ОБЯЗАТЕЛЬНА секция <chunk_size x=\"32\" y=\"16\" z=\"32\"/> сразу после <meta>. \n\
- Каждый <chunk id=\"...\" generator=\"default|axis|none\" chance=\"0..100\">: \n\
   * generator=\"default\" — бесконечная сетка чанков по X/Z. \n\
   * generator=\"axis\" axis=\"x|y|z\" — бесконечная сетка по одной оси. \n\
   * generator=\"none\" — чанк объявлен, но автоматически НЕ создаётся; спавнится вручную из Lua \
через api.chunk.spawn(\"id\", gx, gy, gz). \n\
   * chance — ОБЯЗАТЕЛЬНЫЙ атрибут, 0..100.\n\
- Координаты внутри чанка — ЛОКАЛЬНЫЕ, (0,0,0) = ЦЕНТР чанка. \n\
- Для чанка 32x16x32 пол обычно pos=\"0 -7.9 0\" scale=\"32 0.2 32\".\n\
- ВАЖНО: если в промте передан список файлов из cache-files/, используй ИХ через \
<texture src=\"assets/textures/<имя-файла>\"/> или <model path=\"assets/models/<имя-файла>\"/>. \
Используй именно те имена, что в списке — БЕЗ изменения, БЕЗ добавления пути cache-files/. \
Клиент сам подставит %APPDATA%/.infinity-backrooms/cache-files/ при резолве. \
Если в списке есть .gltf-файл, ссылайся так: <model path=\"assets/models/<имя>.gltf\"/>. \
Если в списке есть PBR-карты (diff, nor_gl, rough, ao, disp, arm) — ссылайся на diff как на \
основную текстуру. \n\
- <prefabs> — глобальные прототипы, сущности внутри чанков наследуют их через inherit=\"pf_id\".\n\
- <resources>: <texture tags=\"...\"/> (резолвится через assets.db/ИИ) либо <texture src=\"...\"/>, \
либо <texture color=\"#rrggbb\"/>, либо инлайн <svg>...</svg>. \n\
- Все Lua-скрипты — инлайн в <scripts>, как <script id=\"...\"><lua>...</lua></script>. \n\
- Уровень должен быть играбельным: минимум один default-чанк с полом и светом, \
один none-чанк-стартовая комната, <spawn_point> указывает на сущность player.\n\
- Обязательно объяви в стартовом none-чанке сущность type=\"player\" id=\"player_start\" с <transform pos=\"0 1 0\"/>.\n\
- Не используй HTML-escape, не оборачивай в CDATA.\n";

/// Собрать ChatRequest для генерации уровня.
pub fn build_level_request(
    number: u32,
    wiki_html: Option<&str>,
    notes: Option<&str>,
    cached_files: &[String],
) -> ChatRequest {
    let mut user = String::new();
    user.push_str(&format!(
        "Сгенерируй уровень номер {number} для Backrooms Infinity.\n"
    ));

    if let Some(html) = wiki_html {
        let trimmed: String = html.chars().take(60_000).collect();
        user.push_str("\nHTML со страницы вики:\n");
        user.push_str(&trimmed);
        user.push('\n');
    }

    if !cached_files.is_empty() {
        user.push_str(
            "\n=== Файлы в cache-files/ (используй их через src= или path=) ===\n",
        );
        for f in cached_files {
            user.push_str("  ");
            user.push_str(f);
            user.push('\n');
        }
        user.push_str(
            "\nСсылайся на них так: \
             <texture src=\"assets/textures/<имя-файла>\"/> для текстур \
             (БЕЗ суффикса резолюции — просто имя как в списке, например Brick_Floor_003_diff.png), \
             <model path=\"assets/models/<имя-файла>.gltf\"/> для моделей. \
             Используй ИМЕННО те файлы, что в списке — не выдумывай новые.\n",
        );
    }

    if let Some(n) = notes {
        if !n.trim().is_empty() {
            user.push_str(&format!(
                "\nЗамечания от прошлой генерации - {}\n",
                n.trim()
            ));
        }
    }

    user.push_str(
        "\nТребование: верни только валидный XML формата BDS Level Format v2 (chunked). \
         Уровень содержит <chunk_size> и <chunks><chunk generator chance>. Все сущности — внутри чанков, \
         координаты локальные ((0,0,0) = центр чанка). Все Lua-скрипты хранятся прямо в XML. \
         Никакого текста вне XML.",
    );

    ChatRequest::new(MODEL_LEVEL_SEARCH)
        .system(SYSTEM_INSTRUCTION)
        .user(user)
}

/// Fallback-запрос через reasoner, если первая модель не дала валидный XML.
pub fn build_level_request_reasoner(
    number: u32,
    wiki_html: Option<&str>,
    notes: Option<&str>,
    cached_files: &[String],
) -> ChatRequest {
    let base = build_level_request(number, wiki_html, notes, cached_files);
    ChatRequest {
        model: MODEL_REASONER.into(),
        ..base
    }
}

/// URL вики-страницы для уровня.
pub fn wiki_url(number: u32) -> String {
    format!("{WIKI_BASE}{number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_url_formats() {
        assert_eq!(wiki_url(0), "https://www.backroomswiki.ru/level-0");
        assert_eq!(wiki_url(998), "https://www.backroomswiki.ru/level-998");
    }

    #[test]
    fn build_request_includes_notes() {
        let r = build_level_request(5, None, Some("больше растений"), &[]);
        let joined: String = r.messages.iter().map(|m| m.content.clone()).collect();
        assert!(joined.contains("Замечания от прошлой генерации - больше растений"));
    }

    #[test]
    fn build_request_includes_html_snippet() {
        let r = build_level_request(1, Some("<html>описание уровня 1</html>"), None, &[]);
        let joined: String = r.messages.iter().map(|m| m.content.clone()).collect();
        assert!(joined.contains("описание уровня 1"));
    }

    #[test]
    fn build_request_includes_cached_files() {
        let files = vec![
            "Brick_Floor_003_diff.png".to_string(),
            "Brick_Floor_003_nor_gl.png".to_string(),
            "Wooden_Chair_01_files".to_string(),
        ];
        let r = build_level_request(2, None, None, &files);
        let joined: String = r.messages.iter().map(|m| m.content.clone()).collect();
        assert!(joined.contains("Brick_Floor_003_diff.png"));
        assert!(joined.contains("cache-files/"));
    }

    #[test]
    fn build_request_uses_search_model() {
        let r = build_level_request(2, None, None, &[]);
        assert_eq!(r.model, MODEL_LEVEL_SEARCH);
    }

    #[test]
    fn reasoner_fallback_uses_reasoner() {
        let r = build_level_request_reasoner(2, None, None, &[]);
        assert_eq!(r.model, MODEL_REASONER);
    }
}
