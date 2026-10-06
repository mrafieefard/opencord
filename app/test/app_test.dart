import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app.dart';

void main() {
  testWidgets('placeholder shows the core version it was given', (
    tester,
  ) async {
    await tester.pumpWidget(
      const OpencordApp(coreVersion: '9.9.9 (protocol v7)'),
    );

    expect(find.text('Opencord'), findsOneWidget);
    expect(find.text('Core 9.9.9 (protocol v7)'), findsOneWidget);
  });
}
