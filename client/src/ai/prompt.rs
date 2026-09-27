//! Сборка промтов для ИИ. Тексты лежат в файлах `prompts/*.txt`
//! и включаются в бинарник через include_str! — их можно менять без правки Rust.

use crate::ai::client::{ChatRequest, MODEL_LEVEL_SEARCH, MODEL_REASONER};

pub const WIKI_BASE: &str = "https://www.backroomswiki.ru/level-";

/// Системная инструкция для Этапа 2 — вынесена в файл.
pub const SYSTEM_INSTRUCTION: &str =
    include_str!("../../prompts/stage2_prompt.txt");

/// Системная инструкция для Этапа 1 — вынесена в файл.
pub const STAGE1_INSTRUCTION: &str =
    include_str!("../../prompts/stage1_prompt.txt");

/// Собрать ChatRequest для генерации уровня (Этап 2).
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
             <texture src=\"assets/textures/<имя-файла>\"/> для текстур, \
             <model path=\"assets/models/<папка>_files/<файл>.gltf\"/> для моделей. \
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
        "\nКРИТИЧЕСКИ ВАЖНО: верни валидный XML формата BDS Level Format v2 (chunked). \
         Уровень ОБЯЗАН содержать none-чанк со сущностью type=\"player\" id=\"player_start\" — \
         без неё игра не запустится (чёрный экран). \
         Все сущности — внутри чанков, координаты локальные. \
         Все Lua-скрипты хранятся прямо в XML. Никакого текста вне XML.",
    );

    ChatRequest::new(MODEL_LEVEL_SEARCH)
        .system(SYSTEM_INSTRUCTION)
        .user(user)
}

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

/// Собрать user-промт для Этапа 1 (какие ассеты нужны).
pub fn build_stage1_user_prompt(
    level_number: u32,
    wiki_html: Option<&str>,
    existing_files: &[String],
    notes: Option<&str>,
) -> String {
    let mut user = String::new();
    user.push_str(&format!(
        "Уровень номер {level_number} для Backrooms Infinity.\n\n"
    ));

    if let Some(html) = wiki_html {
        let trimmed: String = html.chars().take(80_000).collect();
        user.push_str("=== HTML страницы вики ===\n");
        user.push_str(&trimmed);
        user.push_str("\n\n");
    } else {
        user.push_str("(HTML вики недоступен — работай по общим знаниям о backrooms)\n\n");
    }

    user.push_str("=== Файлы, УЖЕ лежащие в cache-files/ ===\n");
    if existing_files.is_empty() {
        user.push_str("(пусто)\n");
    } else {
        for f in existing_files {
            user.push_str("  ");
            user.push_str(f);
            user.push('\n');
        }
    }

    if let Some(n) = notes {
        if !n.trim().is_empty() {
            user.push_str("\nЗамечания от прошлой генерации - ");
            user.push_str(n.trim());
            user.push('\n');
        }
    }

    user.push_str(
        "\nВерни ТОЛЬКО JSON-массив (может быть пустым []) с тем, что ещё нужно скачать. \
         Никакого текста вокруг.",
    );

    user
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
    fn stage2_instruction_loaded_from_file() {
        // Проверяем, что include_str! действительно загрузил текст.
        assert!(SYSTEM_INSTRUCTION.contains("BDS Level Format v2"));
        assert!(SYSTEM_INSTRUCTION.contains("player_start"));
    }

    #[test]
    fn stage1_instruction_loaded_from_file() {
        assert!(STAGE1_INSTRUCTION.contains("JSON-МАССИВОМ")
            || STAGE1_INSTRUCTION.contains("JSON-массив"));
    }

    #[test]
    fn stage1_user_prompt_contains_html_and_files() {
        let p = build_stage1_user_prompt(
            7,
            Some("<html>Level 7 — Thalassophobia</html>"),
            &["Concrete_Floor_diff.png".to_string()],
            None,
        );
        assert!(p.contains("Thalassophobia"));
        assert!(p.contains("Concrete_Floor_diff.png"));
        assert!(p.contains("JSON-массив"));
    }
}
