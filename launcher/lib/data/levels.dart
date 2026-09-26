// Список 999 уровней (§4.1 ТЗ): 0..998, имя совпадает с номером на старте.
// Читается/пишется в levels.json в корне директории данных.

import 'dart:convert';

import 'paths.dart';

class LevelEntry {
  final String name;
  final int number;

  LevelEntry({required this.name, required this.number});

  Map<String, dynamic> toJson() => {'name': name, 'number': number};

  static LevelEntry fromJson(Map<String, dynamic> json) {
    return LevelEntry(
      name: (json['name'] ?? '').toString(),
      number: (json['number'] as num).toInt(),
    );
  }
}

class LevelsStore {
  final AppPaths paths;

  LevelsStore(this.paths);

  /// Всего уровней — 999 (0..998).
  static const int totalLevels = 999;

  /// Формирует список по умолчанию: имена = номера.
  static List<LevelEntry> defaults() {
    return List<LevelEntry>.generate(
      totalLevels,
      (i) => LevelEntry(name: '$i', number: i),
    );
  }

  /// Читает levels.json, при отсутствии/ошибке создаёт файл со значениями по умолчанию.
  Future<List<LevelEntry>> load() async {
    final file = paths.levelsJson;
    if (!await file.exists()) {
      final list = defaults();
      await save(list);
      return list;
    }
    try {
      final raw = await file.readAsString();
      final decoded = jsonDecode(raw);
      if (decoded is List) {
        final list = <LevelEntry>[];
        for (final item in decoded) {
          if (item is Map<String, dynamic>) {
            list.add(LevelEntry.fromJson(item));
          } else if (item is Map) {
            list.add(LevelEntry.fromJson(Map<String, dynamic>.from(item)));
          }
        }
        if (list.length == totalLevels) {
          return list;
        }
      }
    } catch (_) {
      // fall through — перезапишем значением по умолчанию.
    }
    final fresh = defaults();
    await save(fresh);
    return fresh;
  }

  /// Записывает список уровней в levels.json.
  Future<void> save(List<LevelEntry> levels) async {
    await paths.ensure();
    final encoded = jsonEncode(levels.map((l) => l.toJson()).toList());
    await paths.levelsJson.writeAsString(encoded);
  }

  /// Проверяет наличие сгенерированного XML уровня.
  Future<bool> hasLevelXml(int number) {
    return paths.levelXml(number).exists();
  }
}