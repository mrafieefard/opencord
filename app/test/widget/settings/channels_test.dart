import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/server/channels_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _in(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

/// A channel or category in the page's list (the editor's fields come
/// later in the tree).
Finder _tile(String name) => find
    .descendant(of: find.byType(ServerChannelsPage), matching: find.text(name))
    .first;

Future<void> _openChannels(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(_in(find.text('Channels')).first);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _select(WidgetTester tester, String name) async {
  await tester.tap(_tile(name));
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

Future<void> _permissions(WidgetTester tester) async {
  await tester.tap(_in(find.text('Permissions')));
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

Future<void> _drag(WidgetTester tester, Finder from, Offset to) async {
  final start = tester.getCenter(from);
  final gesture = await tester.startGesture(start);
  await tester.pump(const Duration(milliseconds: 100));
  // Reorderable lists follow the pointer in small steps.
  for (var step = 1; step <= 10; step++) {
    await gesture.moveTo(Offset.lerp(start, to, step / 10)!);
    await tester.pump(const Duration(milliseconds: 30));
  }
  await gesture.up();
  await _pumpFor(tester, const Duration(milliseconds: 800));
}

Map<int, Channel> _channels(MockApp app) =>
    app.read(serverProvider(_dev)).data!.channels;

Channel _named(MockApp app, String name) =>
    _channels(app).values.firstWhere((channel) => channel.name == name);

int _roleId(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .roles
    .values
    .firstWhere((role) => role.name == name)
    .id;

void main() {
  testWidgets('channels run in sidebar order under their categories', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);

    final order = [
      'INFORMATION',
      'announcements',
      'welcome',
      'TEXT CHANNELS',
      'general',
      'maintainers',
      'VOICE',
      'General',
    ].map((name) => tester.getTopLeft(_tile(name)).dy).toList();
    for (var i = 1; i < order.length; i++) {
      expect(order[i], greaterThan(order[i - 1]));
    }
    await app.dispose(tester);
  });

  testWidgets('both tabs fit the smallest window at large text', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      size: OcSize.minWindow,
      textScale: 1.5,
    );
    await _openChannels(tester);
    await _select(tester, 'announcements');
    await _permissions(tester);
    await tester.tap(_in(find.widgetWithText(OcButton, 'Add role or member')));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(tester.takeException(), isNull);
    await app.dispose(tester);
  });

  testWidgets('a renamed channel is saved with Save changes', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);
    await _select(tester, 'help');

    await tester.enterText(_in(find.byType(TextField)).first, 'Support Desk');
    await tester.enterText(_in(find.byType(TextField)).at(1), 'Ask away');
    await tester.pump();
    await tester.tap(_in(find.widgetWithText(OcButton, 'Save changes')));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final channel = _named(app, 'support-desk');
    expect(channel.topic, 'Ask away');
    await app.dispose(tester);
  });

  testWidgets('a channel moves to another category from its row', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);
    await _select(tester, 'help');

    await tester.tap(_in(find.text('Category')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.text('Information'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final information = _named(app, 'Information');
    expect(_named(app, 'help').parentId, information.id);
    await app.dispose(tester);
  });

  testWidgets('deleting a channel asks first', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);
    await _select(tester, 'off-topic');

    await tester.tap(_in(find.text('Delete channel')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Delete #off-topic?'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Delete channel'));
    await _pumpFor(tester, const Duration(milliseconds: 800));

    expect(
      _channels(app).values.map((channel) => channel.name),
      isNot(contains('off-topic')),
    );
    expect(find.byType(SettingsDialog), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('dragging a channel reorders it within its category', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);

    await _drag(
      tester,
      _tile('off-topic'),
      tester.getCenter(_tile('general')) - const Offset(0, 12),
    );

    expect(
      _named(app, 'off-topic').position,
      lessThan(_named(app, 'general').position),
    );
    await app.dispose(tester);
  });

  testWidgets('dragging a category moves it with its channels', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);

    await _drag(
      tester,
      _tile('VOICE'),
      tester.getCenter(_tile('INFORMATION')) - const Offset(0, 12),
    );

    expect(
      _named(app, 'Voice').position,
      lessThan(_named(app, 'Information').position),
    );
    expect(
      tester.getTopLeft(_tile('Pairing')).dy,
      lessThan(tester.getTopLeft(_tile('INFORMATION')).dy),
    );
    await app.dispose(tester);
  });

  group('permissions', () {
    testWidgets('Deny on @everyone writes an overwrite at once', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openChannels(tester);
      await _select(tester, 'general');
      await _permissions(tester);

      final deny = find.bySemanticsLabel('Deny Send messages');
      await tester.ensureVisible(deny);
      await tester.tap(deny);
      await _pumpFor(tester, const Duration(milliseconds: 600));

      final everyone = app.read(serverProvider(_dev)).data!.info.everyoneRoleId;
      final overwrite = _named(app, 'general').overwrites.singleWhere(
        (o) => o.targets(OverwriteTargetKind.role, everyone),
      );
      expect(overwrite.deny.has(Permissions.sendMessages), isTrue);
      await app.dispose(tester);
    });

    testWidgets('a role picked from the search gets its own overwrite', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openChannels(tester);
      await _select(tester, 'general');
      await _permissions(tester);

      await tester.tap(
        _in(find.widgetWithText(OcButton, 'Add role or member')),
      );
      await _pumpFor(tester, const Duration(milliseconds: 300));
      await tester.enterText(find.byType(TextField).last, 'contri');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await _pumpFor(tester, const Duration(milliseconds: 800));

      final contributor = _roleId(app, 'Contributor');
      expect(
        _named(app, 'general').overwrites.any(
          (o) => o.targets(OverwriteTargetKind.role, contributor),
        ),
        isTrue,
      );
      expect(_in(find.text('Remove Contributor')), findsOneWidget);
      await app.dispose(tester);
    });

    testWidgets('making a private channel public asks first', (tester) async {
      final app = await MockApp.pump(tester);
      await _openChannels(tester);
      await _select(tester, 'maintainers');
      await _permissions(tester);

      final reset = _in(find.text('Reset to inherit'));
      await tester.ensureVisible(reset);
      await tester.tap(reset);
      await _pumpFor(tester, const Duration(milliseconds: 300));
      expect(
        find.text('Make #maintainers visible to everyone?'),
        findsOneWidget,
      );

      await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
      await _pumpFor(tester, const Duration(milliseconds: 600));
      final everyone = app.read(serverProvider(_dev)).data!.info.everyoneRoleId;
      expect(
        _named(
          app,
          'maintainers',
        ).overwrites.any((o) => o.targets(OverwriteTargetKind.role, everyone)),
        isTrue,
      );
      await app.dispose(tester);
    });

    testWidgets('Remove drops a role overwrite', (tester) async {
      final app = await MockApp.pump(tester);
      await _openChannels(tester);
      await _select(tester, 'announcements');
      await _permissions(tester);

      await tester.tap(_in(find.text('Maintainer')));
      await _pumpFor(tester, const Duration(milliseconds: 200));
      final remove = _in(find.text('Remove Maintainer'));
      await tester.ensureVisible(remove);
      await tester.tap(remove);
      await _pumpFor(tester, const Duration(milliseconds: 600));

      final maintainer = _roleId(app, 'Maintainer');
      expect(
        _named(app, 'announcements').overwrites.any(
          (o) => o.targets(OverwriteTargetKind.role, maintainer),
        ),
        isFalse,
      );
      await app.dispose(tester);
    });
  });

  testWidgets('an unsaved name still asks on close from the other tab', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openChannels(tester);
    await _select(tester, 'help');
    await tester.enterText(_in(find.byType(TextField)).first, 'helpdesk');
    await tester.pump();
    await _permissions(tester);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(find.text('Discard your changes?'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('Edit channel in a channel menu opens it in settings', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('dev-core'),
      ),
      buttons: kSecondaryButton,
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.text('Edit channel'));
    await _pumpFor(tester, const Duration(milliseconds: 400));

    final field = tester.widget<TextField>(_in(find.byType(TextField)).first);
    expect(field.controller!.text, 'dev-core');
    await app.dispose(tester);
  });
}
