import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/channels/server_head.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/home/home_sidebar.dart';
import 'package:opencord/features/home/home_tiles.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/home/request_bar.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/panel_header.dart';

import '../../support/app.dart';

Future<void> _pumpHomeSidebar(WidgetTester tester, MockApp app) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: const Scaffold(
          body: SizedBox(
            width: 288,
            height: 800,
            child: HomeSidebar(),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _pumpChat(
  WidgetTester tester,
  MockApp app,
  String peer, {
  Size size = const Size(800, 700),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(() {
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
  });
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(
          body: SizedBox.fromSize(
            size: size,
            child: PrivateChat(peer: peer),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('tiles_numbers', (tester) async {
    final app = await MockApp.pump(tester);

    // Ensure home shown and friends selected initially
    app.container.read(navigationProvider.notifier).openFriends();
    app.container.read(friendsTabProvider.notifier).select(FriendsTab.online);

    await _pumpHomeSidebar(tester, app);

    // 1. Grid padding 8 8 8 4
    final homeTilesPadding = tester.widget<Padding>(
      find.descendant(
        of: find.byType(HomeTiles),
        matching: find.byWidgetPredicate((w) => w is Padding && w.padding == const EdgeInsets.fromLTRB(8, 8, 8, 4)),
      ),
    );
    expect(homeTilesPadding.padding, const EdgeInsets.fromLTRB(8, 8, 8, 4));

    // Gap between tiles is 8
    final tilesRow = tester.widget<Row>(
      find.descendant(
        of: find.byType(HomeTiles),
        matching: find.byWidgetPredicate((w) => w is Row && w.children.length == 3),
      ),
    );
    final gap = tilesRow.children[1] as SizedBox;
    expect(gap.width, 8.0);

    // 2. Tile numbers
    final tiles = tester.widgetList<HomeTile>(find.byType(HomeTile)).toList();
    expect(tiles.length, 2);

    final friendsTile = tiles[0];
    final requestsTile = tiles[1];
    expect(friendsTile.selected, isTrue);
    expect(requestsTile.selected, isFalse);

    // Check min height 92, padding 10, radius 12, border 1
    final friendsTileContainer = tester.widget<Container>(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Friends'),
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.minHeight == 92 &&
          w.padding == const EdgeInsets.all(10)),
      ),
    );
    expect(friendsTileContainer.constraints?.minHeight, 92);
    final friendsDeco = friendsTileContainer.decoration as BoxDecoration;
    expect(friendsDeco.borderRadius, BorderRadius.circular(12));
    expect(friendsDeco.border?.top.width, 1.0);
    // When selected: accent background, accent border
    expect(friendsDeco.color, OcColors.dark.accent);

    final requestsTileContainer = tester.widget<Container>(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Message requests'),
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.minHeight == 92 &&
          w.padding == const EdgeInsets.all(10)),
      ),
    );
    final requestsDeco = requestsTileContainer.decoration as BoxDecoration;
    expect(requestsDeco.borderRadius, BorderRadius.circular(12));
    expect(requestsDeco.border?.top.width, 1.0);
    expect(requestsDeco.color, OcColors.dark.hover);

    // 3. Icon tile 32, radius 10, background selected (or onAccent when tile selected), icon 20
    final friendsIconBox = tester.widget<Container>(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Friends'),
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.maxWidth == 32 &&
          w.constraints?.maxHeight == 32),
      ),
    );
    final friendsIconDeco = friendsIconBox.decoration as BoxDecoration;
    expect(friendsIconDeco.borderRadius, BorderRadius.circular(10));
    // Inverted on selected tile:
    expect(friendsIconDeco.color, OcColors.dark.onAccent);
    final friendsIcon = tester.widget<Icon>(
      find.descendant(of: find.byWidget(friendsIconBox), matching: find.byType(Icon)),
    );
    expect(friendsIcon.size, 20);
    expect(friendsIcon.color, OcColors.dark.accent);

    // Unselected tile icon box:
    final requestsIconBox = tester.widget<Container>(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Message requests'),
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.maxWidth == 32 &&
          w.constraints?.maxHeight == 32),
      ),
    );
    final requestsIconDeco = requestsIconBox.decoration as BoxDecoration;
    expect(requestsIconDeco.borderRadius, BorderRadius.circular(10));
    expect(requestsIconDeco.color, OcColors.dark.selected);
    final requestsIcon = tester.widget<Icon>(
      find.descendant(of: find.byWidget(requestsIconBox), matching: find.byType(Icon)),
    );
    expect(requestsIcon.size, 20);
    expect(requestsIcon.color, OcColors.dark.text);

    // 4. Text styles: name 13/600, line height 1.2; line 2 11.5
    final friendsNameText = tester.widget<Text>(find.text('Friends'));
    expect(friendsNameText.style?.fontSize, 13);
    expect(friendsNameText.style?.fontWeight, FontWeight.w600);
    expect(friendsNameText.style?.height, 1.2);
    expect(friendsNameText.style?.color, OcColors.dark.onAccent);

    final reqNameText = tester.widget<Text>(find.text('Message requests'));
    expect(reqNameText.style?.fontSize, 13);
    expect(reqNameText.style?.fontWeight, FontWeight.w600);
    expect(reqNameText.style?.height, 1.2);
    expect(reqNameText.style?.color, OcColors.dark.text);

    final reqSubtitleText = tester.widget<Text>(find.text('2 waiting'));
    expect(reqSubtitleText.style?.fontSize, 11.5);
    expect(reqSubtitleText.style?.fontWeight, FontWeight.w400);
    expect(reqSubtitleText.style?.color, OcColors.dark.textMuted);

    // 5. Press scale 0.97
    final scaleFinder = find.descendant(
      of: find.widgetWithText(HomeTile, 'Message requests'),
      matching: find.byType(AnimatedScale),
    );
    expect(tester.widget<AnimatedScale>(scaleFinder).scale, 1.0);

    final gesture = await tester.startGesture(tester.getCenter(find.widgetWithText(HomeTile, 'Message requests')));
    await tester.pump(const Duration(milliseconds: 90));
    expect(tester.widget<AnimatedScale>(scaleFinder).scale, 0.97);
    await gesture.up();
    await tester.pumpAndSettle();

    await app.dispose(tester);
  });

  testWidgets('tiles_count_what_is_waiting', (tester) async {
    final app = await MockApp.pump(tester);
    app.container.read(navigationProvider.notifier).openFriends();

    // Add another non-spam incoming request so incomingRequests count is 2
    final extraKey = fakeFingerprint('key-extra-friend');
    final extraContact = PrivateContact(
      peer: extraKey,
      name: 'Extra Friend',
      friend: FriendStatus.incoming,
      requestSpam: false,
      conversation: ConversationStatus.none,
      relayServerKey: 'opencord.example:7710',
      lastActivity: DateTime.now(),
      unread: 0,
    );
    app.repository.privateWorld.contacts[extraKey] = extraContact;
    app.repository.emit(PrivateContactChanged(extraContact));

    await _pumpHomeSidebar(tester, app);

    // 2 online friends: Kai Nakamura and Mira Okafor
    expect(find.text('2 online'), findsOneWidget);
    // Incoming requests: Jonas Weber + Extra Friend = 2
    expect(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Friends'),
        matching: find.text('2'),
      ),
      findsOneWidget,
    );

    // 2 message requests: Priya Shah + Greta Hoffmann
    expect(find.text('2 waiting'), findsOneWidget);
    expect(
      find.descendant(
        of: find.widgetWithText(HomeTile, 'Message requests'),
        matching: find.text('2'),
      ),
      findsOneWidget,
    );

    // Accept or ignore both message requests so none remain
    final priyaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Priya Shah')
        .peer;
    final gretaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Greta Hoffmann')
        .peer;
    await app.repository.acceptMessageRequest(priyaKey);
    await app.repository.ignoreMessageRequest(gretaKey);
    await tester.pumpAndSettle();

    expect(find.text('All answered'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('tiles_open_their_pages_twice', (tester) async {
    final app = await MockApp.pump(tester);
    app.container.read(navigationProvider.notifier).openFriends();
    app.container.read(friendsTabProvider.notifier).select(FriendsTab.online);

    await _pumpHomeSidebar(tester, app);

    // 1. Initially Friends is selected
    expect(app.container.read(friendsTabProvider), FriendsTab.online);
    expect(app.container.read(homeShownProvider), isTrue);

    // 2. Tap Message requests
    await tester.tap(find.widgetWithText(HomeTile, 'Message requests'));
    await tester.pumpAndSettle();
    expect(app.container.read(friendsTabProvider), FriendsTab.requests);
    expect(app.container.read(homeShownProvider), isTrue);
    expect(app.container.read(currentPeerProvider), isNull);

    // 3. Tap Friends
    await tester.tap(find.widgetWithText(HomeTile, 'Friends'));
    await tester.pumpAndSettle();
    expect(app.container.read(friendsTabProvider), FriendsTab.online);
    expect(app.container.read(homeShownProvider), isTrue);
    expect(app.container.read(currentPeerProvider), isNull);

    // 4. Tap Message requests again
    await tester.tap(find.widgetWithText(HomeTile, 'Message requests'));
    await tester.pumpAndSettle();
    expect(app.container.read(friendsTabProvider), FriendsTab.requests);
    expect(app.container.read(homeShownProvider), isTrue);

    // 5. Tap Friends again
    await tester.tap(find.widgetWithText(HomeTile, 'Friends'));
    await tester.pumpAndSettle();
    expect(app.container.read(friendsTabProvider), FriendsTab.online);
    expect(app.container.read(homeShownProvider), isTrue);

    await app.dispose(tester);
  });

  testWidgets('conversation_row_numbers', (tester) async {
    final app = await MockApp.pump(tester);
    app.container.read(navigationProvider.notifier).openFriends();

    await _pumpHomeSidebar(tester, app);

    // Find row for Kai Nakamura
    final kaiContainerFinder = find.ancestor(
      of: find.text('Kai Nakamura'),
      matching: find.byWidgetPredicate((w) =>
        w is Container &&
        w.constraints?.maxHeight == 44 &&
        w.margin == const EdgeInsets.fromLTRB(8, 0, 8, 2) &&
        w.padding == const EdgeInsets.symmetric(horizontal: 12)),
    );
    expect(kaiContainerFinder, findsOneWidget);
    final kaiContainer = tester.widget<Container>(kaiContainerFinder);
    expect(kaiContainer.constraints?.maxHeight, 44);
    expect(kaiContainer.margin, const EdgeInsets.fromLTRB(8, 0, 8, 2));
    expect(kaiContainer.padding, const EdgeInsets.symmetric(horizontal: 12));
    final deco = kaiContainer.decoration as BoxDecoration;
    expect(deco.borderRadius, BorderRadius.circular(10));

    // ContactAvatar size 32
    final avatar = tester.widget<ContactAvatar>(
      find.descendant(of: kaiContainerFinder, matching: find.byType(ContactAvatar)),
    );
    expect(avatar.size, 32);

    // Gap between avatar and name is 12
    final rowWidget = tester.widget<Row>(
      find.descendant(of: kaiContainerFinder, matching: find.byType(Row)).first,
    );
    final gap = rowWidget.children[1] as SizedBox;
    expect(gap.width, 12.0);

    // Text: 14/500 textSecondary (Kai has 0 unread)
    final nameText = tester.widget<Text>(
      find.descendant(of: kaiContainerFinder, matching: find.text('Kai Nakamura')),
    );
    expect(nameText.style?.fontSize, 14);
    expect(nameText.style?.fontWeight, FontWeight.w500);
    expect(nameText.style?.color, OcColors.dark.textSecondary);

    // Make Kai unread and check 14/600 text
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;
    final updatedKai = app.repository.privateWorld.contacts[kaiKey]!.copyWith(unread: 2);
    app.repository.privateWorld.contacts[kaiKey] = updatedKai;
    app.repository.emit(PrivateContactChanged(updatedKai));
    await tester.pumpAndSettle();

    final unreadNameText = tester.widget<Text>(
      find.descendant(of: kaiContainerFinder, matching: find.text('Kai Nakamura')),
    );
    expect(unreadNameText.style?.fontWeight, FontWeight.w600);

    // Hover shows close button
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.descendant(of: kaiContainerFinder, matching: find.text('Kai Nakamura'))));
    await tester.pumpAndSettle();

    final closeBtn = find.descendant(
      of: kaiContainerFinder,
      matching: find.byWidgetPredicate((w) => w is OcIconButton && w.icon == OcIcons.close),
    );
    expect(closeBtn, findsOneWidget);

    // Right-click opens menu (Close conversation, Block, etc.)
    await tester.tapAt(
      tester.getCenter(find.descendant(of: kaiContainerFinder, matching: find.text('Kai Nakamura'))),
      buttons: kSecondaryMouseButton,
      kind: PointerDeviceKind.mouse,
    );
    await tester.pumpAndSettle();

    expect(find.text('Close conversation'), findsOneWidget);
    expect(find.text('Block'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('chat_header_numbers', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // PanelHeader height 52
    final headerBox = tester.renderObject(find.byType(PanelHeader));
    expect(headerBox.paintBounds.height, 52.0);

    // ContactAvatar size 36, gap 10
    final avatar = tester.widget<ContactAvatar>(
      find.descendant(of: find.byType(PanelHeader), matching: find.byType(ContactAvatar)),
    );
    expect(avatar.size, 36);

    final headerRow = tester.widget<Row>(
      find.descendant(of: find.byType(PanelHeader), matching: find.byType(Row)).first,
    );
    final avatarIndex = headerRow.children.indexWhere((w) => w is ContactAvatar);
    final gap = headerRow.children[avatarIndex + 1] as SizedBox;
    expect(gap.width, 10.0);

    // Lock button
    final lockBtn = find.descendant(
      of: find.byType(PanelHeader),
      matching: find.byWidgetPredicate((w) => w is OcIconButton && (w.icon == OcIcons.lock || w.icon == OcIcons.verifiedUser)),
    );
    expect(lockBtn, findsOneWidget);

    // Relay chip: height 30, padding 0 8, radius 8, 12.5 textSecondary, gap 6, chevron 16
    final relayChipFinder = find.descendant(
      of: find.byType(PanelHeader),
      matching: find.byWidgetPredicate((w) =>
        w is Container &&
        w.constraints?.maxHeight == 30 &&
        w.padding == const EdgeInsets.symmetric(horizontal: 8)),
    );
    expect(relayChipFinder, findsOneWidget);

    final relayContainer = tester.widget<Container>(relayChipFinder);
    final relayDeco = relayContainer.decoration as BoxDecoration;
    expect(relayDeco.borderRadius, BorderRadius.circular(8));
    expect(relayDeco.border?.top.width, 1.0);

    final relayText = tester.widget<Text>(
      find.descendant(of: relayChipFinder, matching: find.byWidgetPredicate((w) => w is Text && (w.data?.startsWith('via ') ?? false))),
    );
    expect(relayText.style?.fontSize, 12.5);
    expect(relayText.style?.color, OcColors.dark.textSecondary);

    final chevronIcon = tester.widget<Icon>(
      find.descendant(of: relayChipFinder, matching: find.byWidgetPredicate((w) => w is Icon && w.icon == OcIcons.expandMore)),
    );
    expect(chevronIcon.size, 16);
    expect(chevronIcon.color, OcColors.dark.textSecondary);

    // Server avatar in chip
    final serverAvatar = find.descendant(of: relayChipFinder, matching: find.byType(ServerAvatar));
    expect(serverAvatar, findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('every_bottom_bar_is_a_float', (tester) async {
    final app = await MockApp.pump(tester);
    final world = app.repository.privateWorld;
    final kaiKey = world.contacts.values.firstWhere((c) => c.name == 'Kai Nakamura').peer;
    final priyaKey = world.contacts.values.firstWhere((c) => c.name == 'Priya Shah').peer;

    // 1. Request bar (Priya Shah has ConversationStatus.request)
    await _pumpChat(tester, app, priyaKey);
    final composerFloat1 = find.byKey(ShellKeys.composer);
    expect(composerFloat1, findsOneWidget);
    expect(tester.getSize(composerFloat1).height, greaterThanOrEqualTo(62.0));
    expect(find.descendant(of: composerFloat1, matching: find.byType(RequestBar)), findsOneWidget);

    // 2. Blocked bar
    world.blockUser(kaiKey);
    app.repository.emit(PrivateContactChanged(world.contacts[kaiKey]!));
    await _pumpChat(tester, app, kaiKey);
    final composerFloat2 = find.byKey(ShellKeys.composer);
    expect(composerFloat2, findsOneWidget);
    expect(tester.getSize(composerFloat2).height, greaterThanOrEqualTo(62.0));
    expect(find.descendant(of: composerFloat2, matching: find.text('You blocked Kai Nakamura.')), findsOneWidget);
    expect(find.descendant(of: composerFloat2, matching: find.widgetWithText(OcButton, 'Unblock')), findsOneWidget);

    // Unblock Kai for next tests
    await app.repository.unblockUser(kaiKey);
    await tester.pumpAndSettle();

    // 3. No shared server: unblock Old Friend, who shares no server
    final oldFriendKey = fakeFingerprint('old-friend');
    await app.repository.unblockUser(oldFriendKey);
    await _pumpChat(tester, app, oldFriendKey);
    final composerFloat3 = find.byKey(ShellKeys.composer);
    expect(composerFloat3, findsOneWidget);
    expect(tester.getSize(composerFloat3).height, greaterThanOrEqualTo(62.0));
    expect(
      find.descendant(of: composerFloat3, matching: find.text('You no longer share a server with Old Friend.')),
      findsOneWidget,
    );

    // 4. Not reachable
    final unreachableKai = world.contacts[kaiKey]!.copyWith(reachable: false);
    world.contacts[kaiKey] = unreachableKai;
    app.repository.emit(PrivateContactChanged(unreachableKai));
    await _pumpChat(tester, app, kaiKey);
    final composerFloat4 = find.byKey(ShellKeys.composer);
    expect(composerFloat4, findsOneWidget);
    expect(tester.getSize(composerFloat4).height, greaterThanOrEqualTo(62.0));
    expect(
      find.descendant(
        of: composerFloat4,
        matching: find.text('Nothing you write would reach Kai Nakamura until you compare the words or use the new keys.'),
      ),
      findsOneWidget,
    );

    // Restore reachable
    final reachableKai = world.contacts[kaiKey]!.copyWith(reachable: true);
    world.contacts[kaiKey] = reachableKai;
    app.repository.emit(PrivateContactChanged(reachableKai));

    // 5. Not connected
    // Disconnect all shared servers
    for (final s in app.container.read(sharedServersProvider(kaiKey))) {
      app.repository.emit(ConnectionChanged(s.serverKey, const ConnectionStatus.connecting()));
    }
    await _pumpChat(tester, app, kaiKey);

    final composerFloat5 = find.byKey(ShellKeys.composer);
    expect(composerFloat5, findsOneWidget);
    expect(tester.getSize(composerFloat5).height, greaterThanOrEqualTo(62.0));
    expect(
      find.descendant(
        of: composerFloat5,
        matching: find.text("You don't share a connected server with Kai Nakamura right now."),
      ),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('head_in_private_is_not_a_button', (tester) async {
    final app = await MockApp.pump(tester);
    app.container.read(navigationProvider.notifier).openFriends();

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: app.container,
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: const Scaffold(
            body: SizedBox(
              width: 288,
              height: 52,
              child: ServerHead(),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final headFloat = find.byKey(ShellKeys.head);
    expect(headFloat, findsOneWidget);
    expect(tester.getSize(headFloat).height, 52.0);

    // Contains 'Private messages' and '<n> online'
    expect(find.text('Private messages'), findsOneWidget);
    expect(find.text('2 online'), findsOneWidget);
    expect(find.byType(ConnectionDot), findsOneWidget);

    // Not a button: no expandMore chevron, Hoverable onTap is null and cursor is basic
    expect(find.byIcon(OcIcons.expandMore), findsNothing);
    expect(find.byType(InkWell), findsNothing);

    final hoverable = tester.widget<Hoverable>(
      find.descendant(of: find.byType(ServerHead), matching: find.byType(Hoverable)),
    );
    expect(hoverable.onTap, isNull);
    expect(hoverable.cursor, SystemMouseCursors.basic);

    await app.dispose(tester);
  });
}
