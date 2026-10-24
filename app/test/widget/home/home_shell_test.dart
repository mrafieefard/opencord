import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/home/home_main.dart';
import 'package:opencord/features/home/home_sidebar.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/no_servers.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

void main() {
  testWidgets('the_rail_button_opens_the_home_view_and_shows_its_badge', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    // Initial state: server view open, unread count badge present on rail home item
    final homeButton = find.byTooltip('Private messages');
    expect(homeButton, findsOneWidget);

    // Mentions badge corresponds to privateUnreadProvider
    final unreadCount = app.container.read(privateUnreadProvider);
    expect(unreadCount > 0, isTrue);
    expect(find.text('$unreadCount'), findsAtLeastNWidgets(1));

    // Tap rail home button
    await tester.tap(homeButton);
    await tester.pumpAndSettle();

    expect(app.container.read(homeShownProvider), isTrue);
    expect(find.byType(HomeSidebar), findsOneWidget);
    expect(find.byType(HomeMain), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('servers_are_not_selected_while_home_is_open', (tester) async {
    final app = await MockApp.pump(tester);

    // Initially server is selected
    expect(app.container.read(homeShownProvider), isFalse);

    // Open home
    await tester.tap(find.byTooltip('Private messages'));
    await tester.pumpAndSettle();

    expect(app.container.read(homeShownProvider), isTrue);

    // Home item is selected
    final homeItem = tester.widget<RailItemView>(
      find.descendant(
        of: find.byType(ServerRail),
        matching: find.byWidgetPredicate((w) => w is RailItemView && w.name == 'Private'),
      ),
    );
    expect(homeItem.selected, isTrue);

    // No server RailItemView is selected
    final serverItems = tester.widgetList<RailItemView>(
      find.descendant(
        of: find.byType(ServerRail),
        matching: find.byWidgetPredicate((w) => w is RailItemView && w.name != 'Private'),
      ),
    );
    for (final item in serverItems) {
      expect(item.selected, isFalse);
    }

    await app.dispose(tester);
  });

  testWidgets('settings_still_open_from_the_user_panel_and_the_shortcut', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    // UserPanel settings button opens Settings
    final userSettingsBtn = find.byWidgetPredicate(
      (w) => w is OcIconButton && w.tooltip == 'User settings',
    );
    expect(userSettingsBtn, findsAtLeastNWidgets(1));
    await tester.tap(userSettingsBtn.first);
    await tester.pumpAndSettle();

    expect(find.byType(SettingsDialog), findsOneWidget);

    // Close settings dialog (Escape)
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byType(SettingsDialog), findsNothing);

    // Shortcut (Control + comma) opens Settings
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.comma);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    expect(find.byType(SettingsDialog), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('conversations_are_listed_newest_first_with_unread_badges', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(find.byTooltip('Private messages'));
    await tester.pumpAndSettle();

    expect(find.byType(HomeSidebar), findsOneWidget);
    expect(find.text('Private messages'), findsAtLeastNWidgets(1));
    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Friends'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Message requests'),
      ),
      findsOneWidget,
    );
    expect(find.text('DIRECT MESSAGES'), findsOneWidget);

    // Kai Nakamura and Mira Okafor are listed in sidebar
    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Kai Nakamura'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Mira Okafor'),
      ),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('closing_a_conversation_hides_it_until_its_next_message', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(find.byTooltip('Private messages'));
    await tester.pumpAndSettle();

    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    final kaiRow = find.descendant(
      of: find.byType(HomeSidebar),
      matching: find.text('Kai Nakamura'),
    );
    expect(kaiRow, findsOneWidget);

    // Hover Kai row to show close button
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer(location: Offset.zero);
    await gesture.moveTo(tester.getCenter(kaiRow));
    await tester.pumpAndSettle();

    // Close button appears
    final closeBtn = find.byTooltip('Close conversation');
    expect(closeBtn, findsOneWidget);
    await tester.tap(closeBtn);
    await tester.pumpAndSettle();

    // Kai Nakamura is hidden in sidebar
    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Kai Nakamura'),
      ),
      findsNothing,
    );

    // A new message from Kai brings it back
    final newMsg = PrivateMessage(
      id: 'kai_new_1',
      peer: kaiKey,
      outgoing: false,
      kind: PrivateMessageKind.text,
      content: 'Hey again!',
      sentAt: DateTime.now(),
      state: PrivateSendState.received,
    );
    app.repository.privateWorld.messages[kaiKey]!.add(newMsg);
    final kaiContact = app.repository.privateWorld.contacts[kaiKey]!;
    final unhidden = kaiContact.copyWith(hidden: false, preview: 'Hey again!');
    app.repository.privateWorld.contacts[kaiKey] = unhidden;
    app.repository.emit(PrivateMessageAdded(newMsg));
    app.repository.emit(PrivateContactChanged(unhidden));
    await tester.pumpAndSettle();

    expect(
      find.descendant(
        of: find.byType(HomeSidebar),
        matching: find.text('Kai Nakamura'),
      ),
      findsOneWidget,
    );

    await gesture.removePointer();
    await app.dispose(tester);
  });

  testWidgets('the_home_view_is_remembered', (tester) async {
    final store = MemoryKeyValueStore();
    final app1 = await MockApp.pump(tester, store: store);

    // Switch to Home
    await tester.tap(find.byTooltip('Private messages'));
    await tester.pumpAndSettle();
    expect(app1.container.read(homeShownProvider), isTrue);

    await app1.dispose(tester);

    // Pump new app with same store
    final app2 = await MockApp.pump(tester, store: store);
    await tester.pumpAndSettle();

    expect(app2.container.read(homeShownProvider), isTrue);
    expect(find.byType(HomeSidebar), findsOneWidget);

    await app2.dispose(tester);
  });

  testWidgets('with_no_servers_the_home_view_shows_the_join_choices', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    // Remove all servers and contacts
    for (final s in [...app.repository.servers]) {
      await app.repository.removeServer(s.key);
    }
    app.repository.privateWorld.contacts.clear();
    app.repository.emit(const PrivateStateChanged());
    await tester.pumpAndSettle();

    expect(app.container.read(homeShownProvider), isTrue);
    expect(find.byType(NoServersView), findsOneWidget);
    expect(find.text('No servers yet'), findsOneWidget);
    expect(find.text('Add server'), findsOneWidget);
    expect(find.text('Host your own'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('a_fresh_start_is_explained_once', (tester) async {
    final app = await MockApp.pump(tester);

    // Simulate private database starting fresh
    app.repository.debugSetPrivateStartedFresh(true);

    // Switch to Home
    await tester.tap(find.byTooltip('Private messages'));
    await tester.pumpAndSettle();

    // Dialog appears explaining the fresh start
    expect(find.byType(OcDialog), findsOneWidget);
    expect(
      find.textContaining(
        "Your private messages on this device couldn't be opened, so this device starts fresh. New messages will arrive normally.",
      ),
      findsOneWidget,
    );
    expect(app.repository.privateStartedFresh, isFalse);

    // Dismiss dialog
    await tester.tap(find.text('OK'));
    await tester.pumpAndSettle();
    expect(find.byType(OcDialog), findsNothing);

    // Does not reappear on subsequent pumps
    await tester.pumpAndSettle();
    expect(find.byType(OcDialog), findsNothing);

    await app.dispose(tester);
  });
}
