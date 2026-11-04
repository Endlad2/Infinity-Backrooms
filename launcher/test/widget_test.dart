import 'package:backrooms_infinity_launcher/main.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('launcher renders the home screen', (WidgetTester tester) async {
    await tester.pumpWidget(const InfinityLauncher());

    expect(find.text('BACKROOMS'), findsOneWidget);
    expect(find.text('INFINITY'), findsOneWidget);
    expect(find.text('Single player'), findsOneWidget);
    expect(find.text('Multiplayer'), findsOneWidget);
  });
}
