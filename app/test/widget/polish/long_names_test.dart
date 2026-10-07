import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';

import '../../support/app.dart';

void main() {
  testWidgets('long channel names end in … and show in full on hover (§16)', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.repository.debugLongNames();
    await tester.pump(const Duration(milliseconds: 300));
    final long = app
        .read(serverProvider('opencord.example:7710'))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name.contains('really-long'))
        .name;

    final tooltips = tester.widgetList<Tooltip>(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.byType(Tooltip),
      ),
    );

    expect(tooltips.map((tooltip) => tooltip.message), contains(long));
    await app.dispose(tester);
  });
}
