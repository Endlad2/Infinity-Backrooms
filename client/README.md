# Backrooms Infinity — Game Client

Rust + Bevy 0.14 клиент игры.

## Двухэтапная генерация уровня

### Этап 1 — Резолв ассетов через Poly Haven (`assets::stage1`)

Запускается **только если `levels/{N}.xml` ещё не существует** (или при `--notes`).

1. Сканируется `%APPDATA%\.infinity-backrooms\cache-files\` — какие файлы уже есть.
2. Локальный ИИ (`localhost:9655`, модель `deepseek-chat`) получает список этих
   файлов и возвращает JSON-массив того, что **ещё нужно** для уровня:
   ```json
   [{"kind":"texture","query":"concrete wall","used_for":"walls","tags":["concrete"]},
    {"kind":"model","query":"wooden chair","used_for":"props"}]
```

3. Для каждого запроса идём в Poly Haven:

- `GET /search?q=...&t=textures|models` → топ-1 slug
- `GET /files/{slug}` → ищем резолюцию 8K и 6 PBR-карт
(`Diffuse`, `nor_gl`, `Rough`, `AO`, `Displacement`, `arm`)
- Скачиваем прямо в `cache-files/` (плоская структура) под именем
`{Asset_Name_Sanitized}_{map}.{ext}` (например `Brick_Floor_003_diff.png`).
- Для моделей: `gltf` + все `include`-файлы в подпапку
`cache-files/{Asset_Name}_files/` с сохранением относительных путей.
4. Если Poly Haven не нашёл ассет — запрос записывается в
`cache-files/_ai_fallback_requests.json` для последующей ИИ-генерации.

### Этап 2 — Генерация XML через ИИ (`level::gen`)

1. Скачивается HTML страницы вики (`backroomswiki.ru/level-{N}`).
2. В промт добавляется **список всех файлов из `cache-files/`**.
3. Модели `deepseek-chat-search` → fallback `deepseek-reasoner`.
4. ИИ пишет `<texture src="assets/textures/Brick_Floor_003_diff.png"/>` —
клиент при резолве сам подставит `%APPDATA%\.infinity-backrooms\cache-files\`.

## Формат файлов в cache-files/

Плоская структура. Пример:

```
cache-files/
├── Aerial_Asphalt_01_diff.png
├── Aerial_Asphalt_01_nor_gl.png
├── Aerial_Asphalt_01_rough.png
├── Aerial_Asphalt_01_ao.png
├── Aerial_Asphalt_01_disp.png
├── Aerial_Asphalt_01_arm.png
├── Brick_Floor_003_diff.png
├── Brick_Floor_003_nor_gl.png
├── ...
├── Wooden_Chair_01_files/
│   ├── Wooden_Chair_01.gltf
│   └── textures/
│       ├── albedo.png
│       └── normal.png
└── _ai_fallback_requests.json
```

Имя ассета берётся из `name` поля Poly Haven и санитизируется:
пробелы → `_`, спецсимволы удаляются (`Aerial Asphalt 01` → `Aerial_Asphalt_01`).

## PBR-набор

Резолвер (`assets::resolver`) автоматически подхватывает соседние карты:

- `diff` → base_color
- `nor_gl` → normal_map
- `rough` → metallic_roughness (канал R)
- `ao` → occlusion
- `disp` → height (пока не используется)
- `arm` → AO/Rough/Metallic упакованные

## Резолюция

По умолчанию качаем **8K**. Позже, при выборе «плохой графики», можно
уменьшить через библиотеки (image/`fast_image_resize`) уже после скачивания.

## Rate-limit

Poly Haven API: при 429/503 — retry с exponential backoff, максимум 3 попытки
(см. `polyhaven.rs::PolyHavenClient`).

## Запуск

```
cargo build --release
# кладём backrooms-client.exe (или client.exe) в %APPDATA%\.infinity-backrooms\
```

## Требования

- Локальный OpenAI-совместимый прокси на `http://localhost:9655/v1/chat/completions`
с моделями `deepseek-chat-search`, `deepseek-chat`, `deepseek-reasoner`.
- Доступ в интернет для Poly Haven API и страниц вики (best-effort, есть offline fallback).

