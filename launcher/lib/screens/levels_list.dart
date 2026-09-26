// Экран «Список уровней» (§4.1 ТЗ): 999 уровней из levels.json.
// При выборе — показывается LoadingScreen с этапами генерации,
// затем запускается client.exe --mode single --level N.

import 'package:flutter/material.dart';

import '../data/levels.dart';
import '../data/paths.dart';
import '../runner.dart';
import '../widgets/blurred_button.dart';
import 'loading_screen.dart';

class LevelsListScreen extends StatefulWidget {
  final AppPaths paths;
  final LevelsStore levelsStore;

  const LevelsListScreen({
    super.key,
    required this.paths,
    required this.levelsStore,
  });

  @override
  State<LevelsListScreen> createState() => _LevelsListScreenState();
}

class _LevelsListScreenState extends State<LevelsListScreen> {
  List<LevelEntry>? _levels;
  final TextEditingController _search = TextEditingController();
  final Set<int> _generated = {};

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final list = await widget.levelsStore.load();
    final gen = <int>{};
    for (final l in list) {
      if (await widget.levelsStore.hasLevelXml(l.number)) {
        gen.add(l.number);
      }
    }
    if (!mounted) return;
    setState(() {
      _levels = list;
      _generated
        ..clear()
        ..addAll(gen);
    });
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _startSingle(int number, String name) async {
    // Показываем экран загрузки, который скрывает UI и запускает клиент.
    await Navigator.of(context).push(
      PageRouteBuilder(
        opaque: false,
        barrierColor: Colors.black,
        pageBuilder: (_, __, ___) => LoadingScreen(
          levelLabel: 'Уровень $name:$number',
          stageLabel: 'Генерация уровня...',
          onReady: () async {
            final result =
                await launchClient(mode: RunMode.single, level: number);
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
    // После возврата обновим статус XML.
    _load();
  }

  @override
  Widget build(BuildContext context) {
    final query = _search.text.trim();
    final list = _levels ?? const <LevelEntry>[];
    final filtered = query.isEmpty
        ? list
        : list
            .where((l) =>
                l.name.toLowerCase().contains(query.toLowerCase()) ||
                l.number.toString().contains(query))
            .toList();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Одиночная игра — уровни'),
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
                hintText: 'Поиск по номеру или имени',
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
      body: _levels == null
          ? const Center(child: CircularProgressIndicator())
          : ListView.builder(
              itemCount: filtered.length,
              itemBuilder: (context, i) {
                final level = filtered[i];
                final hasXml = _generated.contains(level.number);
                return ListTile(
                  leading: CircleAvatar(
                    backgroundColor:
                        hasXml ? Colors.green : const Color(0xFF2A2A2A),
                    child: Text(
                      '${level.number}',
                      style: TextStyle(
                        color: hasXml ? Colors.black : Colors.white,
                        fontSize: 12,
                      ),
                    ),
                  ),
                  title: Text('${level.name}:${level.number}'),
                  subtitle: Text(hasXml ? 'XML готов' : 'Будет сгенерирован ИИ'),
                  trailing: const Icon(Icons.play_arrow),
                  onTap: () => _startSingle(level.number, level.name),
                );
              },
            ),
    );
  }
}
