// Экран выбора роли в мультиплеере (§5.1 ТЗ): «Я хост» / «Я участник».

import 'package:flutter/material.dart';

import '../data/levels.dart';
import '../data/paths.dart';
import 'multiplayer_host.dart';
import 'multiplayer_join.dart';

class MultiplayerRoleScreen extends StatelessWidget {
  final AppPaths paths;
  final LevelsStore levelsStore;

  const MultiplayerRoleScreen({
    super.key,
    required this.paths,
    required this.levelsStore,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Мультиплеер — роль'),
        backgroundColor: const Color(0xFF1B1B1B),
      ),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              SizedBox(
                width: double.infinity,
                height: 64,
                child: ElevatedButton.icon(
                  onPressed: () => Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (_) => MultiplayerHostScreen(
                        paths: paths,
                        levelsStore: levelsStore,
                      ),
                    ),
                  ),
                  icon: const Icon(Icons.wifi_tethering, size: 28),
                  label: const Text('Я хост', style: TextStyle(fontSize: 20)),
                ),
              ),
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                height: 64,
                child: ElevatedButton.icon(
                  onPressed: () => Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (_) => MultiplayerJoinScreen(paths: paths),
                    ),
                  ),
                  icon: const Icon(Icons.login, size: 28),
                  label: const Text('Я участник', style: TextStyle(fontSize: 20)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}