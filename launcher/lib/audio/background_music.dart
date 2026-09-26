// Фоновая музыка: циклично играет assets/back-audio.mp3.
// Использует пакет audioplayers.

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/foundation.dart';

class BackgroundMusic {
  static final BackgroundMusic _instance = BackgroundMusic._internal();
  factory BackgroundMusic() => _instance;
  BackgroundMusic._internal();

  final AudioPlayer _player = AudioPlayer();
  bool _started = false;

  /// Запускает фоновую музыку в цикле.
  Future<void> start() async {
    if (_started) return;
    _started = true;
    try {
      await _player.setReleaseMode(ReleaseMode.loop);
      await _player.setVolume(0.5);
      await _player.play(AssetSource('back-audio.mp3'));
    } catch (e) {
      debugPrint('Не удалось запустить фоновую музыку: $e');
    }
  }

  /// Останавливает музыку.
  Future<void> stop() async {
    try {
      await _player.stop();
    } catch (_) {}
    _started = false;
  }

  Future<void> dispose() async {
    await _player.dispose();
  }
}
