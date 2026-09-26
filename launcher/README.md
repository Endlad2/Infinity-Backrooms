# Backrooms Infinity — Launcher

Flutter-лаунчер для игры Backrooms Infinity.

## Что нового (v0.2.1)

- **Убрана кнопка «Авторы»** из главного меню.
- **Анимированный фон**:
  - зум +25%,
  - плавное движение влево-вправо,
  - смена каждые 10 секунд: `back-1.png` → `back-2.png` → `back-3.png`.
- Всё остальное из v0.2.0 сохранено:
  - вкладка **Credits** со ссылками на Poly Haven, FreeDeepseekApi, BackroomsWikiRu,
  - логотип игры (`logo.png`) в меню,
  - логотип Poly Haven в правом нижнем углу,
  - фоновая музыка (`back-audio.mp3`) циклично,
  - запуск `client.exe` из `%APPDATA%\.infinity-backrooms\`,
  - экран генерации уровня с блюр-панелью и этапами.

## Требуемые ассеты

Положите в папку `assets/`:

- `back-1.png` — фон 1.
- `back-2.png` — фон 2.
- `back-3.png` — фон 3.
- `logo.png` — логотип игры.
- `Poly-Haven-Logo-Black-Full.png` — логотип Poly Haven.
- `back-audio.mp3` — фоновая музыка.

> ⚠️ Старый `back.png` больше не используется — он переименован в `back-1.png`.

## Установка

```bash
flutter clean
flutter pub get
flutter build windows
```

## Структура

- `lib/data/` — данные (уровни, настройки, пути).
- `lib/screens/` — экраны (меню, credits, настройки, мультиплеер, загрузка).
- `lib/widgets/` — `BlurredButton`, `BlurPanel`, `PolyHavenBadge`, `AnimatedBackground`.
- `lib/audio/` — фоновая музыка.
- `lib/runner.dart` — запуск `client.exe` из `%APPDATA%\.infinity-backrooms\`.

## Запуск клиента

Лаунчер ищет `client.exe` в:

```
%APPDATA%\.infinity-backrooms\client.exe
