import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

const _phase1 = RepoCapabilities();
const _dev = 'opencord.example:7710';

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

void main() {
  testWidgets(
    'voice channels say it is coming and do not join (Phase 1 §9.3)',
    (tester) async {
      final app = await MockApp.pump(tester, capabilities: _phase1);

      expect(_inSidebar('Soon'), findsWidgets);
      await tester.tap(_inSidebar('General'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.read(voiceSessionProvider).connected, isFalse);
      expect(
        find.descendant(
          of: find.byType(VoiceView),
          matching: find.text(
            'Voice chat comes in a later version of Opencord.',
          ),
        ),
        findsOneWidget,
      );
      expect(find.text('Join voice'), findsNothing);
      await app.dispose(tester);
    },
  );

  testWidgets('their menu has no Join', (tester) async {
    final app = await MockApp.pump(tester, capabilities: _phase1);

    await tester.tap(_inSidebar('General'), buttons: kSecondaryButton);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final labels = [
      for (final item
          in tester
              .widget<OcMenuPanel>(find.byType(OcMenuPanel))
              .entries
              .whereType<OcMenuItem>())
        item.label,
    ];
    expect(labels, isNot(contains('Join')));
    expect(labels, contains('Open in view'));
    await app.dispose(tester);
  });

  testWidgets('messages offer only what Phase 1 servers do', (tester) async {
    final app = await MockApp.pump(tester, capabilities: _phase1);
    final general = app
        .read(serverProvider(_dev))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name == 'general')
        .id;
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await _pumpFor(tester, const Duration(milliseconds: 800));

    await tester.tap(
      find.textContaining('Thanks').last,
      buttons: kSecondaryButton,
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    final labels = [
      for (final item
          in tester
              .widget<OcMenuPanel>(find.byType(OcMenuPanel))
              .entries
              .whereType<OcMenuItem>())
        item.label,
    ];

    expect(labels, contains('Copy text'));
    expect(labels, isNot(contains('Reply')));
    expect(labels, isNot(contains('Add reaction')));
    expect(labels, isNot(contains('Pin')));
    await app.dispose(tester);
  });
}
