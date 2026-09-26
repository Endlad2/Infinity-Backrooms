// Экран «Я участник» (§5.3 ТЗ): пользователь вводит IP хоста.
// client.exe --mode join --ip {IP}.
// После ввода — LoadingScreen с блюром.

import 'package:flutter/material.dart';

import '../data/paths.dart';
import '../runner.dart';
import 'loading_screen.dart';

class MultiplayerJoinScreen extends StatefulWidget {
  final AppPaths paths;

  const MultiplayerJoinScreen({super.key, required this.paths});

  @override
  State<MultiplayerJoinScreen> createState() => _MultiplayerJoinScreenState();
}

class _MultiplayerJoinScreenState extends State<MultiplayerJoinScreen> {
  final TextEditingController _ip = TextEditingController();
  bool _busy = false;

  @override
  void dispose() {
    _ip.dispose();
    super.dispose();
  }

  Future<void> _join() async {
    final ip = _ip.text.trim();
    if (ip.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Введите IP хоста')),
      );
      return;
    }
    setState(() => _busy = true);
    await Navigator.of(context).push(
      PageRouteBuilder(
        opaque: false,
        barrierColor: Colors.black,
        pageBuilder: (_, __, ___) => LoadingScreen(
          levelLabel: 'Подключение к $ip',
          stageLabel: 'Поиск лобби...',
          onReady: () async {
            final result = await launchClient(mode: RunMode.join, ip: ip);
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
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Участник — ввод IP'),
        backgroundColor: const Color(0xFF1B1B1B),
      ),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                const Text(
                  'Введите IP хоста',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 8),
                const Text(
                  'Хост показал его крупно на своём экране, например 192.168.1.42',
                  textAlign: TextAlign.center,
                  style: TextStyle(fontSize: 14, color: Colors.white70),
                ),
                const SizedBox(height: 24),
                TextField(
                  controller: _ip,
                  autofocus: true,
                  keyboardType: TextInputType.text,
                  style: const TextStyle(color: Colors.white),
                  decoration: InputDecoration(
                    labelText: 'IP хоста',
                    labelStyle: const TextStyle(color: Colors.white70),
                    hintText: '192.168.1.42',
                    hintStyle: const TextStyle(color: Colors.white38),
                    prefixIcon:
                        const Icon(Icons.lan, color: Colors.white70),
                    filled: true,
                    fillColor: Colors.white.withOpacity(0.06),
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(10),
                      borderSide:
                          BorderSide(color: Colors.white.withOpacity(0.2)),
                    ),
                    enabledBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(10),
                      borderSide:
                          BorderSide(color: Colors.white.withOpacity(0.2)),
                    ),
                  ),
                  onSubmitted: (_) => _join(),
                ),
                const SizedBox(height: 24),
                SizedBox(
                  width: double.infinity,
                  height: 52,
                  child: ElevatedButton.icon(
                    onPressed: _busy ? null : _join,
                    icon: _busy
                        ? const SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.login),
                    label: const Text('Подключиться',
                        style: TextStyle(fontSize: 18)),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
