// Главное меню (§4 ТЗ): Мультиплеер, Одиночная игра, Настройки,
// Credits, Выход.
// Фон — анимированный (back-1/2/3.png), кнопки и инпуты — заблюренные,
// логотип сверху.
// Кнопка "Авторы" убрана.

import 'package:flutter/material.dart';

import '../data/levels.dart';
import '../data/paths.dart';
import '../data/settings.dart';
import '../widgets/animated_background.dart';
import '../widgets/blurred_button.dart';
import 'credits.dart';
import 'levels_list.dart';
import 'multiplayer_role.dart';
import 'settings.dart';

class MainMenuScreen extends StatelessWidget {
  final AppPaths paths;
  final LevelsStore levelsStore;
  final SettingsStore settingsStore;

  const MainMenuScreen({
    super.key,
    required this.paths,
    required this.levelsStore,
    required this.settingsStore,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Stack(
        fit: StackFit.expand,
        children: [
          const AnimatedBackground(),
          Container(color: Colors.black.withOpacity(0.35)),
          SafeArea(
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 460),
                child: SingleChildScrollView(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const SizedBox(height: 10),
                      Image.asset(
                        'assets/logo.png',
                        height: 120,
                        fit: BoxFit.contain,
                      ),
                      const SizedBox(height: 34),
                      BlurPanel(
                        child: Column(
                          children: [
                            BlurredButton(
                              label: 'Мультиплеер',
                              icon: Icons.public,
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute(
                                  builder: (_) => MultiplayerRoleScreen(
                                    paths: paths,
                                    levelsStore: levelsStore,
                                  ),
                                ),
                              ),
                            ),
                            BlurredButton(
                              label: 'Одиночная игра',
                              icon: Icons.videogame_asset_outlined,
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute(
                                  builder: (_) => LevelsListScreen(
                                    paths: paths,
                                    levelsStore: levelsStore,
                                  ),
                                ),
                              ),
                            ),
                            BlurredButton(
                              label: 'Настройки',
                              icon: Icons.settings_outlined,
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute(
                                  builder: (_) => SettingsScreen(
                                      settingsStore: settingsStore),
                                ),
                              ),
                            ),
                            BlurredButton(
                              label: 'Credits',
                              icon: Icons.star_outline,
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute(
                                    builder: (_) => const CreditsScreen()),
                              ),
                            ),
                            BlurredButton(
                              label: 'Выход',
                              icon: Icons.exit_to_app,
                              onPressed: () => Navigator.of(context).maybePop(),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 20),
                    ],
                  ),
                ),
              ),
            ),
          ),
          const Positioned(
            right: 14,
            bottom: 14,
            child: PolyHavenBadge(width: 100),
          ),
        ],
      ),
    );
  }
}
