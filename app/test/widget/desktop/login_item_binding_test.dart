import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/desktop/login_item.dart';

import '../../support/app.dart';

class _FakeLoginItem implements LoginItem {
  final calls = <bool>[];

  @override
  Future<void> setEnabled(bool enabled) async => calls.add(enabled);
}

void main() {
  testWidgets('the login item follows the setting from the start', (
    tester,
  ) async {
    final item = _FakeLoginItem();
    final app = await MockApp.pump(
      tester,
      overrides: [loginItemProvider.overrideWithValue(item)],
    );

    expect(item.calls, [false]);

    app
        .read(appSettingsProvider.notifier)
        .update((s) => s.copyWith(launchAtLogin: true));
    await tester.pump();

    expect(item.calls, [false, true]);
    await app.dispose(tester);
  });
}
