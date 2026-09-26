// Точка входа Flutter-лаунчера (Backrooms Infinity, §2.1 ТЗ).
// Регистрирует все экраны, запускает фоновую музыку и открывает главное меню.

import 'package:flutter/material.dart';

import 'audio/background_music.dart';
import 'data/levels.dart';
import 'data/paths.dart';
import 'data/settings.dart';
import 'screens/main_menu.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  final paths = AppPaths.resolve();
  await paths.ensure();

  final levelsStore = LevelsStore(paths);
  await levelsStore.load();

  final settingsStore = SettingsStore(paths);
  await settingsStore.load();

  // Фоновая музыка: циклично играет assets/back-audio.mp3.
  await BackgroundMusic().start();

  runApp(BackroomsLauncherApp(
    paths: paths,
    levelsStore: levelsStore,
    settingsStore: settingsStore,
  ));
}

class BackroomsLauncherApp extends StatelessWidget {
  final AppPaths paths;
  final LevelsStore levelsStore;
  final SettingsStore settingsStore;

  const BackroomsLauncherApp({
    super.key,
    required this.paths,
    required this.levelsStore,
    required this.settingsStore,
  });

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Backrooms Infinity',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.dark,
        primarySwatch: Colors.amber,
        scaffoldBackgroundColor: const Color(0xFF0D0D0D),
        fontFamily: 'Roboto',
      ),
      home: MainMenuScreen(
        paths: paths,
        levelsStore: levelsStore,
        settingsStore: settingsStore,
      ),
    );
  }
}
