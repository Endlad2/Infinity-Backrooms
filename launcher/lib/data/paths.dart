// Пути к данным приложения (§3 ТЗ).
// Windows: %APPDATA%\.infinity-backrooms\
// Linux/macOS: ~/.infinity-backrooms/
// Flutter и Rust используют одну и ту же директорию.

import 'dart:io';

class AppPaths {
  final Directory root;

  AppPaths(this.root);

  /// Определяет корневую директорию данных по переменным окружения.
  static AppPaths resolve() {
    final env = Platform.environment;
    String base;
    if (Platform.isWindows) {
      base = env['APPDATA'] ?? env['USERPROFILE'] ?? '.';
    } else {
      base = env['HOME'] ?? '.';
    }
    return AppPaths(Directory('$base${Platform.pathSeparator}.infinity-backrooms'));
  }

  /// Создаёт все необходимые поддиректории, если их нет.
  Future<void> ensure() async {
    final dirs = <Directory>[
      root,
      Directory(join(root.path, 'levels')),
      Directory(join(root.path, 'assets')),
      Directory(join(join(root.path, 'assets'), 'textures')),
      Directory(join(join(root.path, 'assets'), 'models')),
      Directory(join(join(root.path, 'assets'), 'audio')),
    ];
    for (final d in dirs) {
      if (!await d.exists()) {
        await d.create(recursive: true);
      }
    }
  }

  File get assetsDb => File(join(root.path, 'assets.db'));
  File get levelsJson => File(join(root.path, 'levels.json'));
  File get settingsJson => File(join(root.path, 'settings.json'));
  Directory get levelsDir => Directory(join(root.path, 'levels'));
  Directory get assetsDir => Directory(join(root.path, 'assets'));

  File levelXml(int number) => File(join(levelsDir.path, '$number.xml'));

  /// Простой кроссплатформенный join.
  static String join(String a, String b) {
    final sep = Platform.pathSeparator;
    if (a.endsWith(sep)) return '$a$b';
    return '$a$sep$b';
  }
}