import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/plugin_views.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/plugins/plugin_area.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

import '../../support/app.dart';

void main() {
  const serverKey = 'opencord.example:7710';

  // Fails if opening a plugin channel does not display BlocksView with its root nodes
  // or if tapping a control does not dispatch viewInput and update the view.
  testWidgets('opening_the_channel_shows_the_plugin_and_a_press_counts', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    expect(find.byType(PluginArea), findsOneWidget);
    expect(find.text('Count: 0'), findsOneWidget);
    expect(find.text('Count'), findsOneWidget);

    await tester.tap(find.text('Count'));
    await tester.pumpAndSettle();

    expect(find.text('Count: 1'), findsOneWidget);

    await app.dispose(tester);
  });

  // Fails if each plugin area state (loading, error, too old, closed reasons,
  // unavailable, type gone) does not display its designated message.
  testWidgets('every_state_shows_its_words', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    // 1. Loading state (no batch applied yet)
    app.repository.registerMockView('support-tickets', (viewId) {
      return (input) => null; // Returns null so no roots are set
    });

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byType(OcSpinner), findsOneWidget);

    // 2. Error state
    final controller = app.read(
      pluginViewProvider((server: serverKey, channel: ticketsId)),
    );
    controller.reportError('Failed', 'Detailed crash info');
    await tester.pumpAndSettle();

    expect(find.text('This plugin hit an error'), findsOneWidget);
    expect(find.widgetWithText(OcButton, 'Reload'), findsOneWidget);

    // 3. Too old state
    controller.applyBatch(
      jsonEncode({
        'view': 'test-view',
        'seq': 99,
        'ops': [
          {'op': 'too_old'},
        ],
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('Update Opencord to open this'), findsOneWidget);

    // 4. Closed state: plugin_stopped
    controller.applyBatch(
      jsonEncode({
        'view': 'test-view',
        'seq': 100,
        'ops': [
          {'op': 'closed', 'reason': 'plugin_stopped'},
        ],
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('This plugin stopped'), findsOneWidget);

    // 5. Closed state: lost_access
    controller.applyBatch(
      jsonEncode({
        'view': 'test-view',
        'seq': 101,
        'ops': [
          {'op': 'closed', 'reason': 'lost_access'},
        ],
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('You can no longer see this channel'), findsOneWidget);

    // 6. Closed state: channel_deleted
    controller.applyBatch(
      jsonEncode({
        'view': 'test-view',
        'seq': 102,
        'ops': [
          {'op': 'closed', 'reason': 'channel_deleted'},
        ],
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('This channel was deleted'), findsOneWidget);

    // 7. Closed state: unavailable
    controller.applyBatch(
      jsonEncode({
        'view': 'test-view',
        'seq': 103,
        'ops': [
          {'op': 'closed', 'reason': 'unavailable'},
        ],
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('This channel is unavailable'), findsOneWidget);

    await app.dispose(tester);
  });

  // Fails if error details are shown to standard members without managePlugins
  // permission, or hidden from administrators / owners.
  testWidgets('details_show_only_when_allowed', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    // Normal member: no managePlugins, not owner, no admin
    server.info = ServerInfo(
      serverIdHex: server.info.serverIdHex,
      name: server.info.name,
      ownerId: 999999,
      everyoneRoleId: server.info.everyoneRoleId,
    );
    server.roles[server.info.everyoneRoleId] = Role(
      id: server.info.everyoneRoleId,
      name: '@everyone',
      permissions: Permissions.viewChannel,
      position: 0,
    );
    server.members[server.selfId] = server.members[server.selfId]!.copyWith(
      roleIds: [],
    );
    app.repository.emit(Ready(serverKey, server.snapshot()));
    await tester.pumpAndSettle();

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    final controller = app.read(
      pluginViewProvider((server: serverKey, channel: ticketsId)),
    );
    controller.reportError(
      'Failed',
      'Stack trace: line 42 at /plugins/tickets/main.luau',
    );
    await tester.pumpAndSettle();

    expect(find.text('This plugin hit an error'), findsOneWidget);
    expect(
      find.text('Stack trace: line 42 at /plugins/tickets/main.luau'),
      findsNothing,
    );

    // Admin member: managePlugins permission
    server.roles[server.info.everyoneRoleId] = Role(
      id: server.info.everyoneRoleId,
      name: '@everyone',
      permissions: Permissions.viewChannel | Permissions.managePlugins,
      position: 0,
    );
    app.repository.emit(Ready(serverKey, server.snapshot()));
    await tester.pumpAndSettle();

    expect(
      find.text('Stack trace: line 42 at /plugins/tickets/main.luau'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  // Fails if tapping Reload in the error state does not close and reopen the view.
  testWidgets('reload_reopens_the_view', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    final controller = app.read(
      pluginViewProvider((server: serverKey, channel: ticketsId)),
    );
    controller.reportError('Failed', 'Detailed crash');
    await tester.pumpAndSettle();

    expect(find.text('This plugin hit an error'), findsOneWidget);
    final reloadButton = find.widgetWithText(OcButton, 'Reload');
    expect(reloadButton, findsOneWidget);

    await tester.tap(reloadButton);
    await tester.pumpAndSettle();

    // After reload, the view reopens with initial state (Count: 0)
    expect(find.text('Count: 0'), findsOneWidget);

    await app.dispose(tester);
  });

  // Fails if navigating away from a plugin channel does not call closeView on the repository.
  testWidgets('leaving_the_channel_closes_the_view', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;
    final generalId = server.channels.values
        .firstWhere((c) => c.name == 'general')
        .id;

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    expect(find.byType(PluginArea), findsOneWidget);

    // Switch to general text channel
    app.read(navigationProvider.notifier).openChannel(serverKey, generalId);
    await tester.pumpAndSettle();

    expect(find.byType(PluginArea), findsNothing);

    await app.dispose(tester);
  });

  // Fails if keyboard focus inside the plugin area prevents push-to-talk hotkeys from activating.
  testWidgets('push_to_talk_still_works_with_the_area_focused', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    // Bind grave key to push-to-talk
    app
        .read(keybindsProvider.notifier)
        .add(HotkeyAction.pushToTalk, keys: 'grave');
    await tester.pumpAndSettle();

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    // Focus the plugin area by tapping inside it
    await tester.tap(find.byType(PluginArea));
    await tester.pumpAndSettle();

    expect(app.repository.pushToTalkHeld, isFalse);

    // Press backquote
    await tester.sendKeyDownEvent(LogicalKeyboardKey.backquote);
    await tester.pump();
    expect(app.repository.pushToTalkHeld, isTrue);

    // Release backquote
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backquote);
    await tester.pump();
    expect(app.repository.pushToTalkHeld, isFalse);

    await app.dispose(tester);
  });

  // Fails if a plugin view's open_link request opens immediately without an OcDialog confirmation.
  testWidgets('open_link_asks_first', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;

    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();

    final pluginAreaState = tester.state<PluginAreaState>(
      find.byType(PluginArea),
    );

    // Trigger confirmLink
    final linkFuture = pluginAreaState.blocksHost.confirmLink(
      'https://opencord.dev',
    );
    await tester.pumpAndSettle();

    expect(find.text('Open this link?'), findsOneWidget);
    expect(find.text('https://opencord.dev'), findsOneWidget);

    // Tap Cancel
    await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
    await tester.pumpAndSettle();

    expect(await linkFuture, isFalse);
    expect(find.text('Open this link?'), findsNothing);

    await app.dispose(tester);
  });
}
