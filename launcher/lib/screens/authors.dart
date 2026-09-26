// Экран «Авторы» (§4 ТЗ): благодарности и состав команды.

import 'package:flutter/material.dart';

class AuthorsScreen extends StatelessWidget {
  const AuthorsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Авторы'),
        backgroundColor: const Color(0xFF1B1B1B),
      ),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: const [
                Text(
                  'Backrooms Infinity',
                  style: TextStyle(
                    fontSize: 26,
                    fontWeight: FontWeight.bold,
                    color: Color(0xFFEDD36B),
                  ),
                ),
                SizedBox(height: 8),
                Text(
                  'Фанатский проект во вселенной Backrooms (Закулисье).',
                  style: TextStyle(fontSize: 16, color: Colors.white70),
                ),
                SizedBox(height: 24),
                _SectionTitle('Команда'),
                SizedBox(height: 8),
                Text('• EdgeTypE — идея, дизайн уровней, формат BDS v1'),
                Text('• Rust + Bevy — игровой клиент'),
                Text('• Flutter — лаунчер и меню'),
                Text('• DeepSeek (локальный прокси) — генерация уровней и ассетов'),
                Text('• resvg / tiny-skia — растеризация SVG в PNG'),
                SizedBox(height: 24),
                _SectionTitle('Отдельная благодарность'),
                SizedBox(height: 8),
                Text('Сообществу backroomswiki.ru за материалы о 999 уровнях.'),
                Text('Всем тестировщикам и модераторам.'),
                SizedBox(height: 24),
                _SectionTitle('Технологии'),
                SizedBox(height: 8),
                Text('Rust, Bevy, Flutter, SQLite, Lua (mlua), raw UDP,'),
                Text('OpenAI-совместимый локальный API на localhost:9655.'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  final String text;
  const _SectionTitle(this.text);

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: const TextStyle(
        fontSize: 20,
        fontWeight: FontWeight.bold,
        color: Color(0xFF9DB4FF),
      ),
    );
  }
}