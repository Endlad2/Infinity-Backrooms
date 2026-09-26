# Backrooms Infinity (v3)

3D FPS-игра во вселенной Backrooms. Все 999 уровней генерируются ИИ на основе
данных из вики (`backroomswiki.ru`). Готовые ассеты (текстуры/модели) подтягиваются
из локальной SQLite-БД `assets.db`, а при отсутствии тега — генерируются ИИ
(SVG→PNG для текстур, OBJ-текст для моделей).

## Архитектура

Проект состоит из **двух бинарников**:

| Компонент | Технологии | Назначение |
|---|---|---|
| `launcher/` | Flutter | Главное меню, список уровней, настройки, мультиплеер, запуск Rust-клиента |
| `client/`  | Rust + Bevy | Игровой цикл, генерация уровня, ассеты, Lua-скриптинг, raw-UDP мультиплеер |

## Хранение данных

Все данные в `%APPDATA%\.infinity-backrooms\` (Windows) или `~/.infinity-backrooms/` (Linux/macOS):

```
.infinity-backrooms/
├── assets.db         # SQLite: textures(id, tags, asset), models(id, tags, asset)
├── levels.json       # 999 уровней
├── settings.json     # mouse_sensitivity + bindings
├── levels/           # N.xml — сгенерированные уровни (BDS Level Format v1)
└── assets/           # textures/, models/, audio/
```

## Запуск

### Игровой клиент

```
cd client
cargo run -- --mode single --level 0
cargo run -- --mode host  --level 5
cargo run -- --mode join  --ip 192.168.1.42
```

### Launcher

```
cd launcher
flutter pub get
flutter run
```

## ИИ-бэкенд

Локальный OpenAI-совместимый прокси на `http://localhost:9655` (см. Приложение B ТЗ).

Модели:
- `deepseek-chat-search` — генерация уровней (основная)
- `deepseek-chat` — генерация ассетов (SVG/OBJ)
- `deepseek-reasoner` — сложные случаи / fallback

При недоступности бэкенда клиент использует встроенный offline-уровень-заглушку.

## Управление (по умолчанию)

| Клавиша | Действие |
|---|---|
| W A S D | Передвижение |
| Shift | Бег |
| Ctrl | Присесть (toggle) |
| Space | Прыжок |
| Мышь | Обзор |
| ESC | Пауза |

Все клавиши переназначаемы в `settings.json` (и через Flutter-лаунчер).

## Мультиплеер

Raw UDP на порту **27015**, только LAN, хост — авторитетная сторона.
Пакеты: CONNECT / DISCONNECT / PLAYER_INPUT / WORLD_STATE / EVENT / CHAT.

## Лицензия

См. PROJECT.md.