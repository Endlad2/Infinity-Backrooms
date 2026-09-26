// Экран настроек (§4.2 ТЗ): слайдер чувствительности + таблица биндингов.
// Сохраняет в settings.json через SettingsStore.

import 'package:flutter/material.dart';

import '../data/settings.dart';

class SettingsScreen extends StatefulWidget {
  final SettingsStore settingsStore;

  const SettingsScreen({super.key, required this.settingsStore});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  GameSettings? _settings;
  bool _dirty = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final s = await widget.settingsStore.load();
    if (!mounted) return;
    setState(() => _settings = s);
  }

  Future<void> _save() async {
    final s = _settings;
    if (s == null) return;
    await widget.settingsStore.save(s);
    if (!mounted) return;
    setState(() => _dirty = false);
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Настройки сохранены')),
    );
  }

  @override
  Widget build(BuildContext context) {
    final s = _settings;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Настройки'),
        backgroundColor: const Color(0xFF1B1B1B),
        actions: [
          IconButton(
            icon: const Icon(Icons.save),
            onPressed: _dirty ? _save : null,
            tooltip: 'Сохранить',
          ),
        ],
      ),
      body: s == null
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(16),
              children: [
                const Text('Чувствительность мыши',
                    style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                Row(
                  children: [
                    Expanded(
                      child: Slider(
                        value: s.mouseSensitivity.clamp(
                          GameSettings.minSensitivity,
                          GameSettings.maxSensitivity,
                        ),
                        min: GameSettings.minSensitivity,
                        max: GameSettings.maxSensitivity,
                        divisions: 49,
                        label: s.mouseSensitivity.toStringAsFixed(2),
                        onChanged: (v) {
                          setState(() {
                            s.mouseSensitivity = v;
                            _dirty = true;
                          });
                        },
                      ),
                    ),
                    SizedBox(
                      width: 60,
                      child: Text(
                        s.mouseSensitivity.toStringAsFixed(2),
                        textAlign: TextAlign.right,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 24),
                const Text('Назначение клавиш',
                    style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                const SizedBox(height: 8),
                ...kActionKeys.map((action) => _bindingRow(s, action)),
                const SizedBox(height: 24),
                Center(
                  child: ElevatedButton.icon(
                    onPressed: _dirty ? _save : null,
                    icon: const Icon(Icons.save),
                    label: const Text('Сохранить настройки'),
                  ),
                ),
              ],
            ),
    );
  }

  Widget _bindingRow(GameSettings s, String action) {
    final label = kActionLabels[action] ?? action;
    final key = s.keyFor(action);
    return Card(
      color: const Color(0xFF1B1B1B),
      child: ListTile(
        title: Text(label),
        trailing: OutlinedButton(
          onPressed: () => _rebind(s, action),
          child: Text(key.isEmpty ? '—' : key),
        ),
      ),
    );
  }

  Future<void> _rebind(GameSettings s, String action) async {
    // Простой диалог: ручной ввод клавиши.
    final controller = TextEditingController(text: s.keyFor(action));
    final result = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Клавиша для «${kActionLabels[action] ?? action}»'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(
            hintText: 'W / A / S / D / Space / Shift / Control / E / Escape',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Отмена'),
          ),
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(controller.text.trim()),
            child: const Text('OK'),
          ),
        ],
      ),
    );
    if (result != null && result.isNotEmpty) {
      setState(() {
        s.bindings[action] = result;
        _dirty = true;
      });
    }
  }
}