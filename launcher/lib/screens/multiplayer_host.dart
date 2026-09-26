// Экран «Я хост» (§5.2 ТЗ): тот же список уровней, что и в одиночной игре.
// Формирует команду client.exe --mode host --level {номер}
// с показом LoadingScreen.

import 'package:flutter/material.dart';

import '../data/levels.dart';
import '../data/paths.dart';
import '../runner.dart';
import 'loading_screen.dart';

class MultiplayerHostScreen extends StatefulWidget {
  final AppPaths paths;
  final LevelsStore levelsStore;

  const MultiplayerHostScreen({
    super.key,
    required this.paths,
    required this.levelsStore,
  });

  @override
  State<MultiplayerHostScreen> createState() => _MultiplayerHostScreenState();
}

class _MultiplayerHostScreenState extends State<MultiplayerHostScreen> {
  List<LevelEntry>? _levels;
  final TextEditingController _search = TextEditingController();
  int? _selected;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final list = await widget.levelsStore.load();
    if (!mounted) return;
    setState(() => _levels = list);
  }

  Future<void> _startHost() async {
    final level = _selected;
    if (level == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Выберите уровень')),
      );
      return;
    }
    final name = _levels
            ?.firstWhere((l) => l.number == level,
                orElse: () => LevelEntry(name: '$level', number: level))
            .name ??
        '$level';
    await Navigator.of(context).push(
      PageRouteBuilder(
        opaque: false,
        barrierColor: Colors.black,
        pageBuilder: (_, __, ___) => LoadingScreen(
          levelLabel: 'Хост — уровень $name:$level',
          stageLabel: 'Создание лобби...',
          onReady: () async {
            final result = await launchClient(mode: RunMode.host, level: level);
            if (!result.ok && mounted) {
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text(result.message)),
              );
            }
          },
        ),
        transitionsBuilder: (_, anim, __, child) =>
            FadeTransition(opacity: anim, child: child),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final list = _levels ?? const <LevelEntry>[];
    final query = _search.text.trim();
    final filtered = query.isEmpty
        ? list
        : list
            .where((l) =>
                l.name.toLowerCase().contains(query.toLowerCase()) ||
                l.number.toString().contains(query))
            .toList();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Хост — выбор уровня'),
        backgroundColor: const Color(0xFF1B1B1B),
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: TextField(
              controller: _search,
              onChanged: (_) => setState(() {}),
              style: const TextStyle(color: Colors.white),
              decoration: InputDecoration(
                hintText: 'Поиск уровня',
                hintStyle: const TextStyle(color: Colors.white54),
                prefixIcon: const Icon(Icons.search, color: Colors.white70),
                isDense: true,
                filled: true,
                fillColor: Colors.white.withOpacity(0.06),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(10),
                  borderSide: BorderSide(color: Colors.white.withOpacity(0.2)),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(10),
                  borderSide: BorderSide(color: Colors.white.withOpacity(0.2)),
                ),
              ),
            ),
          ),
        ),
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _selected == null ? null : _startHost,
        icon: const Icon(Icons.wifi_tethering),
        label: const Text('Создать лобби'),
      ),
      body: _levels == null
          ? const Center(child: CircularProgressIndicator())
          : ListView.builder(
              itemCount: filtered.length,
              itemBuilder: (context, i) {
                final level = filtered[i];
                final selected = level.number == _selected;
                return ListTile(
                  selected: selected,
                  leading: CircleAvatar(
                    backgroundColor: selected
                        ? const Color(0xFFEDD36B)
                        : const Color(0xFF2A2A2A),
                    child: Text(
                      '${level.number}',
                      style: TextStyle(
                        color: selected ? Colors.black : Colors.white,
                        fontSize: 12,
                      ),
                    ),
                  ),
                  title: Text('${level.name}:${level.number}'),
                  onTap: () => setState(() => _selected = level.number),
                );
              },
            ),
    );
  }
}
