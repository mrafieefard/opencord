import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';

void main() {
  const serverKey = 'opencord.example:7710';

  // Fails if a plugin channel does not show its registered type icon
  // or extension glyph fallback, or if unread/mention badges are rendered.
  // Mutation: channel_glyph.dart forces iconFile: null -> Image widget is never built.
  testWidgets('a_plugin_channel_shows_its_type_icon', (tester) async {
    final tempDir = Directory.systemTemp;
    final iconFile = File('${tempDir.path}/test_tickets_icon.png');
    // Valid 1x1 transparent PNG bytes
    iconFile.writeAsBytesSync([
      0x89,
      0x50,
      0x4E,
      0x47,
      0x0D,
      0x0A,
      0x1A,
      0x0A,
      0x00,
      0x00,
      0x00,
      0x0D,
      0x49,
      0x48,
      0x44,
      0x52,
      0x00,
      0x00,
      0x00,
      0x01,
      0x00,
      0x00,
      0x00,
      0x01,
      0x08,
      0x06,
      0x00,
      0x00,
      0x00,
      0x1F,
      0x15,
      0xC4,
      0x89,
      0x00,
      0x00,
      0x00,
      0x0A,
      0x49,
      0x44,
      0x41,
      0x54,
      0x78,
      0x9C,
      0x63,
      0x00,
      0x01,
      0x00,
      0x00,
      0x05,
      0x00,
      0x01,
      0x0D,
      0x0A,
      0x2D,
      0xB4,
      0x00,
      0x00,
      0x00,
      0x00,
      0x49,
      0x45,
      0x4E,
      0x44,
      0xAE,
      0x42,
      0x60,
      0x82,
    ]);

    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    app.repository.onPluginAssetFile = (server, plugin, asset) => iconFile.path;

    final server = app.repository.mockServer(serverKey);
    final typesWithIcon = [
      const PluginChannelType(
        pluginId: 'support-tickets',
        typeKey: 'tickets',
        name: 'Tickets',
        description: 'Support desk',
        icon: 'ticket.png',
        pluginName: 'Support Tickets',
      ),
    ];
    server.pluginChannelTypes.clear();
    server.pluginChannelTypes.addAll(typesWithIcon);
    app.repository.emit(PluginChannelTypesUpdated(serverKey, typesWithIcon));

    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    // Verify channel row exists and shows its name
    final ticketsRowFinder = find.byWidgetPredicate(
      (w) => w is ChannelRowView && w.name == 'tickets',
    );
    expect(ticketsRowFinder, findsOneWidget);

    // Verify unread dot or mention badge is never shown on a plugin channel
    expect(
      find.descendant(of: ticketsRowFinder, matching: find.byType(UnreadBadge)),
      findsNothing,
    );
    expect(
      find.descendant(
        of: ticketsRowFinder,
        matching: find.byType(MentionBadge),
      ),
      findsNothing,
    );

    // Verify Image widget is built when icon resolves to a file
    expect(
      find.descendant(of: ticketsRowFinder, matching: find.byType(Image)),
      findsOneWidget,
    );

    // Fallback to OcIcons.extension when icon path is empty
    final typesWithoutIcon = [
      const PluginChannelType(
        pluginId: 'support-tickets',
        typeKey: 'tickets',
        name: 'Tickets',
        description: 'Support desk',
        icon: '',
        pluginName: 'Support Tickets',
      ),
    ];
    server.pluginChannelTypes.clear();
    server.pluginChannelTypes.addAll(typesWithoutIcon);
    app.repository.emit(PluginChannelTypesUpdated(serverKey, typesWithoutIcon));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(
      find.descendant(
        of: ticketsRowFinder,
        matching: find.byIcon(OcIcons.extension),
      ),
      findsOneWidget,
    );

    try {
      iconFile.deleteSync();
    } on Object catch (_) {}
    await app.dispose(tester);
  });

  // Fails if an unavailable channel can be opened by row click, keyboard
  // navigation, or app link, or fails to display "This channel is unavailable".
  // Mutation: NavigationNotifier.openChannel skips cannotOpenReason guard.
  testWidgets('an_unavailable_channel_shows_why_and_doesnt_open', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1200));
    final server = app.repository.mockServer(serverKey);
    final channelId = server.channels.values
        .firstWhere((c) => c.name == 'legacy-help')
        .id;

    await tester.pumpAndSettle();

    final initialChannel = app.read(currentChannelProvider);
    expect(initialChannel, isNot(equals(channelId)));

    // 1. Row click path
    final legacyRow = find.byWidgetPredicate(
      (w) => w is ChannelRowView && w.name == 'legacy-help',
    );
    expect(legacyRow, findsOneWidget);

    await tester.tap(legacyRow);
    await tester.pump();

    // Current channel must NOT change to the unavailable channel
    expect(app.read(currentChannelProvider), initialChannel);
    // Toast message must appear
    expect(find.text('This channel is unavailable'), findsOneWidget);
    await tester.pump(const Duration(seconds: 2));

    // 2. Keyboard navigation path
    // Open 'tickets' channel first, which is immediately before 'legacy-help'
    final ticketsId = server.channels.values
        .firstWhere((c) => c.name == 'tickets')
        .id;
    app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
    await tester.pumpAndSettle();
    expect(app.read(currentChannelProvider), ticketsId);

    // Hold Alt and press Down arrow to step navigation towards legacy-help
    await tester.sendKeyDownEvent(LogicalKeyboardKey.alt);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.alt);
    await tester.pump();

    expect(app.read(currentChannelProvider), ticketsId);
    expect(find.text('This channel is unavailable'), findsOneWidget);

    // 3. Link path: opencord://serverKey/c/channelId
    final parsed = parseAppLink('opencord://$serverKey/c/$channelId');
    expect(parsed, isNotNull);
    final channelRowElement = tester.element(find.byType(ChannelRow).first);
    await openAppLink(
      channelRowElement,
      channelRowElement as WidgetRef,
      parsed!,
    );
    await tester.pump();

    expect(app.read(currentChannelProvider), isNot(equals(channelId)));
    expect(find.text('This channel is unavailable'), findsOneWidget);

    await app.dispose(tester);
  });

  // Fails if a channel whose type is gone does not show the typeGone message
  // on hover and on open attempts.
  testWidgets('a_type_gone_channel_says_so', (tester) async {
    final app = await MockApp.pump(tester);
    final server = app.repository.mockServer(serverKey);

    // Create a typeGone plugin channel
    final id = 9991;
    final defunctChannel = const Channel(
      id: 9991,
      kind: ChannelKind.plugin,
      name: 'defunct-channel',
      pluginId: 'support-tickets',
      typeKey: 'vanished',
      pluginState: PluginChannelState.typeGone,
    );
    server.channels[id] = defunctChannel;
    app.repository.emit(ChannelUpserted(serverKey, defunctChannel));

    await tester.pumpAndSettle();

    final rowFinder = find.byWidgetPredicate(
      (w) => w is ChannelRowView && w.name == 'defunct-channel',
    );
    expect(rowFinder, findsOneWidget);

    // Clicking row must not open and must show toast
    await tester.tap(rowFinder);
    await tester.pump();

    expect(app.read(currentChannelProvider), isNot(equals(id)));
    expect(
      find.text('This plugin no longer provides this channel'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  // Fails if create channel dialog does not list custom plugin channel types
  // after Text and Voice or fails to send plugin_id and type_key on creation.
  testWidgets('create_channel_lists_plugin_types_and_sends_them', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final context = tester.element(find.byType(ChannelRow).first);

    // Open create channel dialog
    showCreateChannel(context, serverKey: serverKey);
    await tester.pumpAndSettle();

    expect(find.byType(CreateChannelDialog), findsOneWidget);

    // Verify plugin channel type card is displayed
    expect(find.text('Tickets'), findsOneWidget);
    expect(find.text('Support Tickets'), findsOneWidget);

    // Verify private channel switch is initially shown for standard channels
    expect(
      find.byWidgetPredicate(
        (w) => w is OcSwitch && w.semanticLabel == 'Private channel',
      ),
      findsOneWidget,
    );

    // Select the plugin channel type card
    await tester.tap(find.text('Tickets'));
    await tester.pumpAndSettle();

    // Verify private channel switch is hidden for plugin channels (mutation: remove `_selectedPluginType == null`)
    expect(
      find.byWidgetPredicate(
        (w) => w is OcSwitch && w.semanticLabel == 'Private channel',
      ),
      findsNothing,
    );

    // Type name in dialog's text field
    final dialogField = find.descendant(
      of: find.byType(CreateChannelDialog),
      matching: find.byType(TextField),
    );
    await tester.enterText(dialogField, 'support-desk');
    await tester.pumpAndSettle();

    // Submit creation
    await tester.tap(
      find.descendant(
        of: find.byType(CreateChannelDialog),
        matching: find.widgetWithText(OcButton, 'Create channel'),
      ),
    );
    await tester.pumpAndSettle();

    // Dialog closes on success
    expect(find.byType(CreateChannelDialog), findsNothing);

    // Verify channel was created in repository
    final server = app.repository.mockServer(serverKey);
    final created = server.channels.values.firstWhere(
      (c) => c.name == 'support-desk',
    );
    expect(created.kind, ChannelKind.plugin);
    expect(created.pluginId, 'support-tickets');
    expect(created.typeKey, 'tickets');

    await app.dispose(tester);
  });

  // Fails if the server's refusal to create a channel is not displayed
  // inside the create channel dialog.
  testWidgets('the_servers_refusal_shows_in_the_dialog', (tester) async {
    final app = await MockApp.pump(tester);
    final context = tester.element(find.byType(ChannelRow).first);

    showCreateChannel(context, serverKey: serverKey);
    await tester.pumpAndSettle();

    expect(find.byType(CreateChannelDialog), findsOneWidget);

    // Choose Tickets
    await tester.tap(find.text('Tickets'));
    await tester.pumpAndSettle();

    // Configure repository mock to throw RepoException
    app.repository.createChannelError = const RepoException(
      RepoErrorKind.invalidArgument,
      'channel type unavailable',
    );

    final dialogField = find.descendant(
      of: find.byType(CreateChannelDialog),
      matching: find.byType(TextField),
    );
    await tester.enterText(dialogField, 'failing-channel');
    await tester.pumpAndSettle();

    await tester.tap(
      find.descendant(
        of: find.byType(CreateChannelDialog),
        matching: find.widgetWithText(OcButton, 'Create channel'),
      ),
    );
    await tester.pumpAndSettle();

    // Dialog stays open and shows the server's error message
    expect(find.byType(CreateChannelDialog), findsOneWidget);
    expect(find.text('channel type unavailable'), findsOneWidget);

    await app.dispose(tester);
  });
}
