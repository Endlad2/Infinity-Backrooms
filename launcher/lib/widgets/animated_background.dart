// Анимированный фон: зум +25%, плавное движение влево-вправо,
// смена каждые 10 секунд между back-1.png, back-2.png, back-3.png.

import 'dart:async';

import 'package:flutter/material.dart';

class AnimatedBackground extends StatefulWidget {
  /// Список ассетов фона (по порядку).
  final List<String> images;

  /// Интервал смены фона.
  final Duration interval;

  /// Насколько сильно зум (1.25 = +25%).
  final double zoom;

  const AnimatedBackground({
    super.key,
    this.images = const [
      'assets/back-1.png',
      'assets/back-2.png',
      'assets/back-3.png',
    ],
    this.interval = const Duration(seconds: 10),
    this.zoom = 1.25,
  });

  @override
  State<AnimatedBackground> createState() => _AnimatedBackgroundState();
}

class _AnimatedBackgroundState extends State<AnimatedBackground>
    with SingleTickerProviderStateMixin {
  late final AnimationController _panController;
  Timer? _swapTimer;
  int _index = 0;
  bool _showFirst = true;

  @override
  void initState() {
    super.initState();

    // Плавное движение влево-вправо (маятник, 8 сек в одну сторону).
    _panController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 8),
    )..repeat(reverse: true);

    // Смена фона каждые N секунд.
    _swapTimer = Timer.periodic(widget.interval, (_) {
      if (!mounted) return;
      setState(() {
        _index = (_index + 1) % widget.images.length;
        _showFirst = !_showFirst;
      });
    });
  }

  @override
  void dispose() {
    _swapTimer?.cancel();
    _panController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _panController,
      builder: (context, _) {
        final t = _panController.value; // 0..1
        final dx = (t - 0.5) * 24.0; // -12..+12 пикселей

        return ClipRect(
          child: Transform.translate(
            offset: Offset(dx, 0),
            child: Transform.scale(
              scale: widget.zoom,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  AnimatedOpacity(
                    duration: const Duration(milliseconds: 900),
                    opacity: _showFirst ? 1 : 0,
                    child: Image.asset(
                      widget.images[_index],
                      fit: BoxFit.cover,
                    ),
                  ),
                  AnimatedOpacity(
                    duration: const Duration(milliseconds: 900),
                    opacity: _showFirst ? 0 : 1,
                    child: Image.asset(
                      widget.images[(_index + 1) % widget.images.length],
                      fit: BoxFit.cover,
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
