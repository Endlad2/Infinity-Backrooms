// Экран «Credits»: благодарности + ссылки на внешние ресурсы.
// Powered by Poly Haven, FreeDeepseekApi, BackroomsWikiRu.

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../widgets/animated_background.dart';
import '../widgets/blurred_button.dart';

class CreditsScreen extends StatelessWidget {
  const CreditsScreen({super.key});

  Future<void> _open(BuildContext context, String url) async {
    final uri = Uri.parse(url);
    try {
      final ok = await launchUrl(uri, mode: LaunchMode.externalApplication);
      if (!ok && context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Не удалось открыть $url')),
        );
      }
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Ошибка: $e')),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Stack(
        fit: StackFit.expand,
        children: [
          const AnimatedBackground(),
          Container(color: Colors.black.withOpacity(0.55)),
          SafeArea(
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: Row(
                    children: [
                      IconButton(
                        onPressed: () => Navigator.of(context).maybePop(),
                        icon: const Icon(Icons.arrow_back,
                            color: Colors.white, size: 28),
                        tooltip: 'Назад',
                      ),
                    ],
                  ),
                ),
                Expanded(
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 520),
                      child: SingleChildScrollView(
                        padding: const EdgeInsets.symmetric(horizontal: 24),
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            const Text(
                              'CREDITS',
                              style: TextStyle(
                                fontSize: 30,
                                fontWeight: FontWeight.bold,
                                letterSpacing: 4,
                                color: Color(0xFFEDD36B),
                              ),
                            ),
                            const SizedBox(height: 28),
                            BlurPanel(
                              child: Column(
                                children: [
                                  const Text(
                                    'Powered by',
                                    style: TextStyle(
                                      color: Colors.white70,
                                      fontSize: 15,
                                      letterSpacing: 1.2,
                                    ),
                                  ),
                                  const SizedBox(height: 18),
                                  BlurredButton(
                                    label: 'Poly Haven',
                                    icon: Icons.image_outlined,
                                    onPressed: () => _open(
                                        context, 'https://polyhaven.com/'),
                                  ),
                                  BlurredButton(
                                    label: 'FreeDeepseekApi',
                                    icon: Icons.smart_toy_outlined,
                                    onPressed: () => _open(
                                      context,
                                      'https://github.com/ForgetMeAI/FreeDeepseekAPI',
                                    ),
                                  ),
                                  BlurredButton(
                                    label: 'BackroomsWikiRu',
                                    icon: Icons.menu_book_outlined,
                                    onPressed: () => _open(
                                        context, 'https://www.backroomswiki.ru'),
                                  ),
                                ],
                              ),
                            ),
                            const SizedBox(height: 24),
                            Image.asset(
                              'assets/Poly-Haven-Logo-Black-Full.png',
                              width: 140,
                              opacity: const AlwaysStoppedAnimation(0.7),
                            ),
                            const SizedBox(height: 24),
                            const Text(
                              'Backrooms Infinity',
                              style: TextStyle(
                                fontSize: 18,
                                fontWeight: FontWeight.bold,
                                color: Colors.white70,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ],
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
