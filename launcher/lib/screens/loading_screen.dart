// Экран, который показывается после выбора уровня (одиночная игра / хост).
// Прячет весь UI меню, оставляет анимированный фон + логотип игры снизу,
// а посередине — блюр-панель с этапом генерации уровня.

import 'dart:async';

import 'package:flutter/material.dart';

import '../widgets/animated_background.dart';
import '../widgets/blurred_button.dart';

class LoadingScreen extends StatefulWidget {
  final String stageLabel;
  final Future<void> Function() onReady;
  final String levelLabel;

  const LoadingScreen({
    super.key,
    required this.levelLabel,
    required this.stageLabel,
    required this.onReady,
  });

  @override
  State<LoadingScreen> createState() => _LoadingScreenState();
}

class _LoadingScreenState extends State<LoadingScreen> {
  final List<String> _stages = [
    'Инициализация...',
    'Запрос к локальному ИИ...',
    'Генерация геометрии уровня...',
    'Генерация ассетов...',
    'Растеризация SVG в PNG...',
    'Сборка XML уровня...',
    'Запуск клиента...',
  ];
  int _index = 0;
  Timer? _timer;
  bool _launching = false;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(milliseconds: 900), (t) {
      if (!mounted) return;
      setState(() {
        if (_index < _stages.length - 1) _index++;
      });
    });
    _run();
  }

  Future<void> _run() async {
    await Future.delayed(const Duration(milliseconds: 400));
    try {
      await widget.onReady();
    } catch (_) {}
    if (!mounted) return;
    setState(() => _launching = true);
    await Future.delayed(const Duration(milliseconds: 600));
    if (mounted) {
      Navigator.of(context).maybePop();
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Stack(
        fit: StackFit.expand,
        children: [
          // Анимированный фон — весь UI меню скрыт.
          const AnimatedBackground(),
          Container(color: Colors.black.withOpacity(0.45)),

          // Центральная блюр-панель с этапом генерации.
          Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 460),
              child: BlurPanel(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const SizedBox(
                      width: 34,
                      height: 34,
                      child: CircularProgressIndicator(
                        strokeWidth: 3,
                        valueColor: AlwaysStoppedAnimation(Color(0xFFEDD36B)),
                      ),
                    ),
                    const SizedBox(height: 20),
                    Text(
                      widget.levelLabel,
                      style: const TextStyle(
                        color: Colors.white,
                        fontSize: 18,
                        fontWeight: FontWeight.bold,
                        letterSpacing: 1,
                      ),
                    ),
                    const SizedBox(height: 10),
                    AnimatedSwitcher(
                      duration: const Duration(milliseconds: 350),
                      child: Text(
                        _launching ? 'Запуск...' : _stages[_index],
                        key: ValueKey(_launching ? 'launch' : _index),
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          color: Colors.white70,
                          fontSize: 15,
                        ),
                      ),
                    ),
                    const SizedBox(height: 18),
                    ClipRRect(
                      borderRadius: BorderRadius.circular(6),
                      child: LinearProgressIndicator(
                        value: (_index + 1) / _stages.length,
                        minHeight: 6,
                        backgroundColor: Colors.white12,
                        valueColor: const AlwaysStoppedAnimation(
                          Color(0xFFEDD36B),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),

          // Логотип игры снизу.
          Positioned(
            left: 0,
            right: 0,
            bottom: 28,
            child: Center(
              child: Image.asset(
                'assets/logo.png',
                height: 90,
                fit: BoxFit.contain,
              ),
            ),
          ),

          // PolyHaven логотип в нижнем углу.
          const Positioned(
            right: 14,
            bottom: 14,
            child: PolyHavenBadge(width: 90),
          ),
        ],
      ),
    );
  }
}
