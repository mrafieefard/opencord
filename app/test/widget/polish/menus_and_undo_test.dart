import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';
import '../../support/clipboard.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

/// The labels of the open context menu, top to bottom.
List<String> _menuLabels(WidgetTester tester) => [
  for (final item
      in tester
          .widgetList<OcMenuPanel>(find.byType(OcMenuPanel))
          .single
          .entries
          .whereType<OcMenuItem>())
    item.label,
];

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<void> _openServerSettings(WidgetTester tester, String page) async {
  await tester.tap(_inSidebar('Opencord Dev'));
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(_inSettings(find.text(page)).first);
  await _pumpFor(tester, const Duration(milliseconds: 600));
}

void main() {
  group('context menus (§4, §16)', () {
    testWidgets('the rail menu runs as §4.1 lists it', (tester) async {
      final app = await MockApp.pump(tester);

      await tester.tap(
        find.descendant(
          of: find.byKey(DesktopShell.railKey),
          matching: find.text('Opencord Dev'),
        ),
        buttons: kSecondaryButton,
      );
      await _pumpFor(tester, const Duration(milliseconds: 200));

      expect(_menuLabels(tester), [
        'Mark as read',
        'Mute notifications',
        'Invite people',
        'Server settings',
        'Copy address',
        'Leave server',
      ]);
      await app.dispose(tester);
    });

    testWidgets('a voice channel can be edited from its menu', (tester) async {
      final app = await MockApp.pump(tester);

      await tester.tap(_inSidebar('General'), buttons: kSecondaryButton);
      await _pumpFor(tester, const Duration(milliseconds: 200));
      expect(_menuLabels(tester), [
        'Join',
        'Open in view',
        'Copy link',
        'Edit channel',
        'Delete channel',
      ]);
      await tester.tap(find.text('Edit channel'));
      await _pumpFor(tester, const Duration(milliseconds: 400));

      expect(find.byType(SettingsDialog), findsOneWidget);
      await app.dispose(tester);
    });

    testWidgets('a role’s menu copies its ID', (tester) async {
      final clipboard = ClipboardSpy(tester);
      final app = await MockApp.pump(tester);
      await _openServerSettings(tester, 'Roles');
      final contributor = app
          .read(serverProvider(_dev))
          .data!
          .roles
          .values
          .firstWhere((role) => role.name == 'Contributor');

      await tester.tap(
        _inSettings(find.text('Contributor')).first,
        buttons: kSecondaryButton,
      );
      await _pumpFor(tester, const Duration(milliseconds: 200));
      await tester.tap(find.text('Copy role ID'));
      await _pumpFor(tester, const Duration(milliseconds: 200));

      expect(clipboard.copied, '${contributor.id}');
      await app.dispose(tester);
    });

    testWidgets('an invite’s menu copies its link', (tester) async {
      final clipboard = ClipboardSpy(tester);
      final app = await MockApp.pump(tester);
      final created = app.repository.createInvite(_dev);
      await _pumpFor(tester, const Duration(milliseconds: 600));
      final code = (await created).code;
      await _openServerSettings(tester, 'Invites');

      await tester.tap(_inSettings(find.text(code)), buttons: kSecondaryButton);
      await _pumpFor(tester, const Duration(milliseconds: 200));
      expect(_menuLabels(tester), ['Copy link', 'Copy code', 'Revoke invite']);
      await tester.tap(find.text('Copy link'));
      await _pumpFor(tester, const Duration(milliseconds: 200));

      expect(clipboard.copied, startsWith('opencord://$_dev/invite/$code'));
      await app.dispose(tester);
    });
  });

  group('undo instead of confirm (§16)', () {
    testWidgets('leaving voice can be undone', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      await tester.tap(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.bySemanticsLabel('Disconnect'),
        ),
      );
      await _pumpFor(tester, const Duration(milliseconds: 300));
      expect(app.read(voiceSessionProvider).connected, isFalse);
      expect(find.text('Left General'), findsOneWidget);

      await tester.tap(find.text('Undo'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(
        app.read(voiceSessionProvider).channelId,
        _channel(app, 'General'),
      );
      await _pumpFor(tester, const Duration(seconds: 6));
      await app.dispose(tester);
    });

    testWidgets('removing your reaction can be undone', (tester) async {
      final app = await MockApp.pump(tester);
      app
          .read(navigationProvider.notifier)
          .openChannel(_dev, _channel(app, 'general'));
      await _pumpFor(tester, const Duration(milliseconds: 800));
      final mine = find.byWidgetPredicate(
        (widget) => widget is ReactionChip && widget.reaction.me,
      );
      // The newest one, on screen at the bottom of the channel.
      final chip = tester.widget<ReactionChip>(mine.last);
      final emoji = chip.reaction.emoji;

      await tester.tap(mine.last);
      await _pumpFor(tester, const Duration(milliseconds: 600));
      expect(find.text('Reaction removed'), findsOneWidget);

      await tester.tap(find.text('Undo'));
      await _pumpFor(tester, const Duration(milliseconds: 600));

      expect(
        tester
            .widgetList<ReactionChip>(find.byType(ReactionChip))
            .any((chip) => chip.reaction.emoji == emoji && chip.reaction.me),
        isTrue,
      );
      await _pumpFor(tester, const Duration(seconds: 6));
      await app.dispose(tester);
    });
  });
}
