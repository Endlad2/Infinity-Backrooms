// Настройки игрока (§4.2 ТЗ): чувствительность мыши + биндинги.
// Сохраняется в settings.json, читается обоими бинарниками.
// Формат должен совпадать с client/src/settings.rs (Settings).

import 'dart:convert';
import 'dart:io';

import 'paths.dart';

/// Действия для биндингов (совпадают с Rust Bindings).
const List<String> kActionKeys = <String>[
  'move_forward',
  'move_back',
  'move_left',
  'move_right',
  'sprint',
  'crouch',
  'jump',
  'interact',
  'pause',
];

/// Человеко-читаемые названия действий.
const Map<String, String> kActionLabels = <String, String>{
  'move_forward': 'Вперёд',
  'move_back': 'Назад',
  'move_left': 'Влево',
  'move_right': 'Вправо',
  'sprint': 'Бег',
  'crouch': 'Присесть',
  'jump': 'Прыжок',
  'interact': 'Взаимодействие',
  'pause': 'Пауза',
};

/// Настройки игры.
class GameSettings {
  double mouseSensitivity;
  Map<String, String> bindings;

  GameSettings({
    required this.mouseSensitivity,
    required this.bindings,
  });

  static const double minSensitivity = 0.1;
  static const double maxSensitivity = 5.0;

  /// Значения по умолчанию (§7.1 ТЗ).
  factory GameSettings.defaults() {
    return GameSettings(
      mouseSensitivity: 1.0,
      bindings: <String, String>{
        'move_forward': 'W',
        'move_back': 'S',
        'move_left': 'A',
        'move_right': 'D',
        'sprint': 'Shift',
        'crouch': 'Control',
        'jump': 'Space',
        'interact': 'E',
        'pause': 'Escape',
      },
    );
  }

  factory GameSettings.fromJson(Map<String, dynamic> json) {
    final sens = (json['mouse_sensitivity'] as num?)?.toDouble() ?? 1.0;
    final Map<String, String> binds = {};
    final rawBinds = json['bindings'];
    if (rawBinds is Map) {
      rawBinds.forEach((k, v) {
        binds[k.toString()] = v.toString();
      });
    }
    final s = GameSettings(mouseSensitivity: sens, bindings: binds);
    s.fillMissingBindings();
    s.clampSensitivity();
    return s;
  }

  Map<String, dynamic> toJson() => {
        'mouse_sensitivity': mouseSensitivity,
        'bindings': bindings,
      };

  /// Обрезает чувствительность в допустимый диапазон.
  void clampSensitivity() {
    if (mouseSensitivity < minSensitivity) mouseSensitivity = minSensitivity;
    if (mouseSensitivity > maxSensitivity) mouseSensitivity = maxSensitivity;
  }

  /// Дополняет отсутствующие биндинги значениями по умолчанию.
  void fillMissingBindings() {
    final defs = GameSettings.defaults().bindings;
    for (final key in kActionKeys) {
      bindings.putIfAbsent(key, () => defs[key] ?? '');
    }
  }

  /// Возвращает клавишу для действия.
  String keyFor(String action) => bindings[action] ?? '';
}

/// Хранилище настроек в settings.json.
class SettingsStore {
  final AppPaths paths;

  SettingsStore(this.paths);

  /// Читает settings.json; при отсутствии создаёт с дефолтами.
  Future<GameSettings> load() async {
    final file = paths.settingsJson;
    if (!await file.exists()) {
      final s = GameSettings.defaults();
      await save(s);
      return s;
    }
    try {
      final raw = await file.readAsString();
      final decoded = jsonDecode(raw);
      if (decoded is Map<String, dynamic>) {
        return GameSettings.fromJson(decoded);
      } else if (decoded is Map) {
        return GameSettings.fromJson(Map<String, dynamic>.from(decoded));
      }
    } catch (_) {
      // повредился — перезапишем.
    }
    final fresh = GameSettings.defaults();
    await save(fresh);
    return fresh;
  }

  /// Записывает настройки в settings.json.
  Future<void> save(GameSettings settings) async {
    await paths.ensure();
    settings.clampSensitivity();
    settings.fillMissingBindings();
    final encoded = const JsonEncoder.withIndent('  ')
        .convert(settings.toJson());
    await paths.settingsJson.writeAsString(encoded);
  }
}

/// Кроссплатформенный File для settings.json (используется только если нужно явно).
File settingsFile(AppPaths paths) => paths.settingsJson;