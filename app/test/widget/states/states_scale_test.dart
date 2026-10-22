import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

import '../../support/app.dart';

void main() {
  testWidgets('no servers fits the smallest window at large text (§9)', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      size: OcSize.minWindow,
      textScale: 1.5,
    );
    app.repository.debugEmptyServerList();
    await tester.pump(const Duration(milliseconds: 300));

    expect(find.text('No servers yet'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await app.dispose(tester);
  });

  testWidgets('the unreachable banner fits the smallest window at large text', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      size: OcSize.minWindow,
      textScale: 1.5,
    );
    app.read(navigationProvider.notifier).openServer('homelab.local:7710');
    await tester.pump(const Duration(milliseconds: 600));

    expect(find.textContaining("Can't reach server"), findsOneWidget);
    expect(tester.takeException(), isNull);
    await app.dispose(tester);
  });
}
