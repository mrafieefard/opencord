import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/navigation_history.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/title_crumb.dart';
import 'package:opencord/features/window/app_title_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

ServerData _serverData(String key, String name, List<Channel> channelList) =>
    ServerData(
      self: const User(id: 1000, displayName: 'Alex'),
      info: ServerInfo(name: name, ownerId: 1000, everyoneRoleId: 1),
      channels: {for (final c in channelList) c.id: c},
      roles: const {
        1: Role(
          id: 1,
          name: '@everyone',
          position: 0,
          permissions: Permissions.all,
        ),
      },
      members: {
        1000: Member(
          user: const User(id: 1000, displayName: 'Alex'),
          joinedAt: DateTime.utc(2026),
        ),
      },
      serverPermissions: Permissions.all,
      channelPermissions: {for (final c in channelList) c.id: Permissions.all},
    );

class _TestServerListNotifier extends ServerListNotifier {
  _TestServerListNotifier(this._servers);
  List<ServerSummary> _servers;

  @override
  List<ServerSummary> build() => _servers;

  void setServers(List<ServerSummary> servers) {
    _servers = servers;
    state = servers;
  }
}

class _TestServerNotifier extends ServerNotifier {
  _TestServerNotifier(super.serverKey, this._state);
  ServerState _state;

  @override
  ServerState build() => _state;

  void setServerState(ServerState newState) {
    _state = newState;
    state = newState;
  }
}

class _TestPrivateContactsNotifier extends Notifier<PrivateContactsState>
    implements PrivateContactsNotifier {
  _TestPrivateContactsNotifier(this.initialContacts);
  final List<PrivateContact> initialContacts;

  @override
  PrivateContactsState build() {
    return PrivateContactsState(
      byPeer: {for (final c in initialContacts) c.peer: c},
      loaded: true,
    );
  }

  void setContacts(List<PrivateContact> contacts) {
    state = PrivateContactsState(
      byPeer: {for (final c in contacts) c.peer: c},
      loaded: true,
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

Widget _wrap(Widget child, ProviderContainer container) {
  return UncontrolledProviderScope(
    container: container,
    child: MaterialApp(
      theme: buildTheme(OcColors.dark),
      home: Scaffold(body: child),
    ),
  );
}

void main() {
  const serverKey = 'opencord-dev';
  const serverName = 'Opencord Dev';

  const channels = [
    Channel(id: 1, kind: ChannelKind.text, name: 'dev-core', position: 0),
    Channel(id: 2, kind: ChannelKind.voice, name: 'General', position: 1),
    Channel(
      id: 3,
      kind: ChannelKind.announcement,
      name: 'announcements',
      position: 2,
    ),
  ];

  late MemoryKeyValueStore store;
  late ProviderContainer container;

  setUp(() {
    store = MemoryKeyValueStore();
    container = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(store),
        serverListProvider.overrideWith(
          () => _TestServerListNotifier([
            const ServerSummary(key: serverKey, name: serverName),
          ]),
        ),
        serverProvider(serverKey).overrideWith(
          () => _TestServerNotifier(
            serverKey,
            ServerState(
              key: serverKey,
              data: _serverData(serverKey, serverName, channels),
            ),
          ),
        ),
        privateContactsProvider.overrideWith(
          () => _TestPrivateContactsNotifier([
            const PrivateContact(
              peer: 'peer-mira',
              name: 'Mira Okafor',
              friend: FriendStatus.friends,
              conversation: ConversationStatus.accepted,
            ),
          ]),
        ),
      ],
    );
    container.read(navigationHistoryProvider);
  });

  tearDown(() => container.dispose());

  testWidgets('arrows_disable_at_the_ends', (tester) async {
    final nav = container.read(navigationProvider.notifier);
    nav.openChannel(serverKey, 1);

    await tester.pumpWidget(
      _wrap(const AppTitleBar(leading: HistoryArrows()), container),
    );
    await tester.pump();

    final arrows = find.byType(OcIconButton);
    expect(arrows, findsNWidgets(2));

    var backButton = tester.widget<OcIconButton>(arrows.first);
    var forwardButton = tester.widget<OcIconButton>(arrows.last);

    // Both disabled at initial single entry
    expect(backButton.onPressed, isNull);
    expect(forwardButton.onPressed, isNull);
    expect(backButton.disabledColor, OcColors.dark.iconDisabled);
    expect(forwardButton.disabledColor, OcColors.dark.iconDisabled);

    // Add another channel
    nav.openChannel(serverKey, 2);
    await tester.pump();

    backButton = tester.widget<OcIconButton>(arrows.first);
    forwardButton = tester.widget<OcIconButton>(arrows.last);

    expect(backButton.onPressed, isNotNull);
    expect(forwardButton.onPressed, isNull);
  });

  testWidgets('arrow_tooltip_names_the_target', (tester) async {
    final nav = container.read(navigationProvider.notifier);
    final history = container.read(navigationHistoryProvider.notifier);

    // Step 0: #dev-core
    nav.openChannel(serverKey, 1);
    // Step 1: Mira Okafor
    nav.openConversation('peer-mira');
    // Step 2: General (voice)
    nav.openChannel(serverKey, 2);
    // Step 3: Friends
    nav.openFriends();
    // Step 4: Message requests
    container.read(friendsTabProvider.notifier).select(FriendsTab.requests);

    await tester.pumpWidget(
      _wrap(const AppTitleBar(leading: HistoryArrows()), container),
    );
    await tester.pump();

    final arrows = find.byType(OcIconButton);
    var backBtn = tester.widget<OcIconButton>(arrows.first);
    var fwdBtn = tester.widget<OcIconButton>(arrows.last);

    // At step 4 (Message requests): back points to Friends
    expect(backBtn.tooltip, 'Back to Friends');
    expect(fwdBtn.tooltip, 'Forward');

    // Go back to Friends (step 3)
    history.back();
    await tester.pump();
    backBtn = tester.widget<OcIconButton>(arrows.first);
    fwdBtn = tester.widget<OcIconButton>(arrows.last);
    expect(backBtn.tooltip, 'Back to General · Opencord Dev');
    expect(fwdBtn.tooltip, 'Forward to Message requests');

    // Go back to General (voice, step 2)
    history.back();
    await tester.pump();
    backBtn = tester.widget<OcIconButton>(arrows.first);
    fwdBtn = tester.widget<OcIconButton>(arrows.last);
    expect(backBtn.tooltip, 'Back to Mira Okafor');
    expect(fwdBtn.tooltip, 'Forward to Friends');

    // Go back to Mira Okafor (step 1)
    history.back();
    await tester.pump();
    backBtn = tester.widget<OcIconButton>(arrows.first);
    fwdBtn = tester.widget<OcIconButton>(arrows.last);
    expect(backBtn.tooltip, 'Back to #dev-core · Opencord Dev');
    expect(fwdBtn.tooltip, 'Forward to General · Opencord Dev');

    // Go back to #dev-core (step 0)
    history.back();
    await tester.pump();
    backBtn = tester.widget<OcIconButton>(arrows.first);
    fwdBtn = tester.widget<OcIconButton>(arrows.last);
    expect(backBtn.tooltip, 'Back');
    expect(fwdBtn.tooltip, 'Forward to Mira Okafor');
  });

  testWidgets('mouse_side_buttons_step and alt_arrows_step', (tester) async {
    final app = await MockApp.pump(tester);
    try {
      final nav = app.container.read(navigationProvider.notifier);
      final serverKey = app.container.read(serverListProvider).first.key;
      final serverState = app.container.read(serverProvider(serverKey));
      final textChannels = serverState.data!.channels.values
          .where((c) => c.kind == ChannelKind.text)
          .map((c) => c.id)
          .toList();
      final ch1 = textChannels[0];
      final ch2 = textChannels[1];

      nav.openChannel(serverKey, ch1);
      await tester.pumpAndSettle();
      nav.openChannel(serverKey, ch2);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch2);

      // Mouse back button (button 4, kBackMouseButton = 8)
      tester.binding.handlePointerEvent(
        const PointerDownEvent(
          position: Offset(300, 300),
          buttons: kBackMouseButton,
          kind: PointerDeviceKind.mouse,
        ),
      );
      tester.binding.handlePointerEvent(
        const PointerUpEvent(
          position: Offset(300, 300),
          kind: PointerDeviceKind.mouse,
        ),
      );
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch1);

      // Mouse forward button (button 5, kForwardMouseButton = 16)
      tester.binding.handlePointerEvent(
        const PointerDownEvent(
          position: Offset(300, 300),
          buttons: kForwardMouseButton,
          kind: PointerDeviceKind.mouse,
        ),
      );
      tester.binding.handlePointerEvent(
        const PointerUpEvent(
          position: Offset(300, 300),
          kind: PointerDeviceKind.mouse,
        ),
      );
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch2);

      // Keyboard Alt+Left
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch1);

      // Keyboard Alt+Right
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch2);
    } finally {
      await app.dispose(tester);
    }
  });

  testWidgets('macos cmd_bracket steps', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.macOS);
    try {
      final nav = app.container.read(navigationProvider.notifier);
      final serverKey = app.container.read(serverListProvider).first.key;
      final serverState = app.container.read(serverProvider(serverKey));
      final textChannels = serverState.data!.channels.values
          .where((c) => c.kind == ChannelKind.text)
          .map((c) => c.id)
          .toList();
      final ch1 = textChannels[0];
      final ch2 = textChannels[1];

      nav.openChannel(serverKey, ch1);
      await tester.pumpAndSettle();
      nav.openChannel(serverKey, ch2);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch2);

      // ⌘[ (MetaLeft + BracketLeft)
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.bracketLeft);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch1);

      // ⌘] (MetaLeft + BracketRight)
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.bracketRight);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpAndSettle();

      expect(app.container.read(currentChannelProvider), ch2);
    } finally {
      await app.dispose(tester);
    }
  });

  testWidgets('crumb_shows_server_and_channel', (tester) async {
    final nav = container.read(navigationProvider.notifier);

    // 1. Text channel
    nav.openChannel(serverKey, 1);
    await tester.pumpWidget(
      _wrap(const AppTitleBar(center: TitleCrumb()), container),
    );
    await tester.pump();

    expect(find.byKey(ShellKeys.crumb), findsOneWidget);
    expect(find.text('Opencord Dev'), findsOneWidget);
    expect(find.text('/'), findsOneWidget);
    expect(find.text('dev-core'), findsOneWidget);

    // Check ServerAvatar measurement: 20 px, radius 6
    final serverAvatar = tester.widget<ServerAvatar>(find.byType(ServerAvatar));
    expect(serverAvatar.size, 20);
    expect(serverAvatar.borderRadius, 6);

    // 2. Announcement channel
    nav.openChannel(serverKey, 3);
    await tester.pump();
    expect(find.text('announcements'), findsOneWidget);

    // 3. Voice channel
    nav.openChannel(serverKey, 2);
    await tester.pump();
    expect(find.text('General'), findsOneWidget);

    // 4. Private chat
    nav.openConversation('peer-mira');
    await tester.pump();
    expect(find.text('Private messages'), findsOneWidget);
    expect(find.text('Mira Okafor'), findsOneWidget);

    // 5. Friends
    nav.openFriends();
    await tester.pump();
    expect(find.text('Private messages'), findsOneWidget);
    expect(find.text('Friends'), findsOneWidget);

    // 6. Requests
    container.read(friendsTabProvider.notifier).select(FriendsTab.requests);
    await tester.pump();
    expect(find.text('Private messages'), findsOneWidget);
    expect(find.text('Message requests'), findsOneWidget);

    // 7. No server and no contacts -> empty
    (container.read(serverListProvider.notifier) as _TestServerListNotifier)
        .setServers([]);
    (container.read(privateContactsProvider.notifier)
            as _TestPrivateContactsNotifier)
        .setContacts([]);
    await tester.pump();
    expect(find.byKey(ShellKeys.crumb), findsNothing);
  });

  testWidgets('crumb_is_centred_and_clips', (tester) async {
    const longServer =
        'A Very Long Server Name That Extends Far Beyond Normal Widths';
    const longChannel =
        'a-very-long-channel-name-that-also-extends-quite-a-bit';

    final localContainer = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(store),
        serverListProvider.overrideWith(
          () => _TestServerListNotifier([
            const ServerSummary(key: 'long', name: longServer),
          ]),
        ),
        serverProvider('long').overrideWith(
          () => _TestServerNotifier(
            'long',
            ServerState(
              key: 'long',
              data: _serverData('long', longServer, [
                const Channel(
                  id: 99,
                  kind: ChannelKind.text,
                  name: longChannel,
                  position: 0,
                ),
              ]),
            ),
          ),
        ),
        privateContactsProvider.overrideWith(
          () => _TestPrivateContactsNotifier([]),
        ),
      ],
    );
    addTearDown(localContainer.dispose);

    localContainer.read(navigationHistoryProvider);
    localContainer.read(navigationProvider.notifier).openChannel('long', 99);

    tester.view.physicalSize = const Size(800, 600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      _wrap(const AppTitleBar(center: TitleCrumb()), localContainer),
    );
    await tester.pump();

    final crumbRect = tester.getRect(find.byKey(ShellKeys.crumb));
    // Must be inside 56 % of window width (800 * 0.56 = 448)
    expect(crumbRect.width, lessThanOrEqualTo(448.0));
    // Centred in 800 px window: centre around 400
    expect(crumbRect.center.dx, closeTo(400.0, 1.0));
  });
}
