import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/navigation_history.dart';

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

void main() {
  late MemoryKeyValueStore store;
  late ProviderContainer container;

  const serverA = 'server-a';
  const serverB = 'server-b';

  const channelsA = [
    Channel(id: 10, kind: ChannelKind.text, name: 'general', position: 0),
    Channel(id: 11, kind: ChannelKind.text, name: 'dev', position: 1),
    Channel(id: 12, kind: ChannelKind.voice, name: 'Lounge', position: 2),
  ];

  const channelsB = [
    Channel(id: 20, kind: ChannelKind.text, name: 'welcome', position: 0),
    Channel(id: 21, kind: ChannelKind.text, name: 'random', position: 1),
  ];

  setUp(() {
    store = MemoryKeyValueStore();
    container = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(store),
        serverListProvider.overrideWith(
          () => _TestServerListNotifier([
            const ServerSummary(key: serverA, name: 'Server A'),
            const ServerSummary(key: serverB, name: 'Server B'),
          ]),
        ),
        serverProvider(serverA).overrideWith(
          () => _TestServerNotifier(
            serverA,
            ServerState(
              key: serverA,
              data: _serverData(serverA, 'Server A', channelsA),
            ),
          ),
        ),
        serverProvider(serverB).overrideWith(
          () => _TestServerNotifier(
            serverB,
            ServerState(
              key: serverB,
              data: _serverData(serverB, 'Server B', channelsB),
            ),
          ),
        ),
      ],
    );
    // Keep NavigationHistory active
    container.read(navigationHistoryProvider);
  });

  tearDown(() => container.dispose());

  test('opening_channels_builds_history', () {
    final nav = container.read(navigationProvider.notifier);

    nav.openChannel(serverA, 10);
    nav.openChannel(serverA, 11);

    final state = container.read(navigationHistoryProvider);
    expect(state.entries, [
      const ChannelEntry(serverA, 10),
      const ChannelEntry(serverA, 11),
    ]);
    expect(state.index, 1);
    expect(state.canBack, isTrue);
    expect(state.canForward, isFalse);
    expect(state.backTarget, const ChannelEntry(serverA, 10));
  });

  test('back_then_forward_returns', () {
    final nav = container.read(navigationProvider.notifier);
    final history = container.read(navigationHistoryProvider.notifier);

    nav.openChannel(serverA, 10);
    nav.openChannel(serverA, 11);

    history.back();

    var state = container.read(navigationHistoryProvider);
    expect(state.index, 0);
    expect(state.canBack, isFalse);
    expect(state.canForward, isTrue);
    expect(state.forwardTarget, const ChannelEntry(serverA, 11));
    expect(container.read(currentChannelProvider), 10);

    history.forward();

    state = container.read(navigationHistoryProvider);
    expect(state.index, 1);
    expect(state.canBack, isTrue);
    expect(state.canForward, isFalse);
    expect(container.read(currentChannelProvider), 11);
  });

  test('going_somewhere_new_drops_what_is_ahead', () {
    final nav = container.read(navigationProvider.notifier);
    final history = container.read(navigationHistoryProvider.notifier);

    nav.openChannel(serverA, 10);
    nav.openChannel(serverA, 11);
    history.back();

    expect(container.read(navigationHistoryProvider).index, 0);

    nav.openChannel(serverB, 20);

    final state = container.read(navigationHistoryProvider);
    expect(state.entries, [
      const ChannelEntry(serverA, 10),
      const ChannelEntry(serverB, 20),
    ]);
    expect(state.index, 1);
    expect(state.canBack, isTrue);
    expect(state.canForward, isFalse);
  });

  test('reopening_the_same_place_adds_nothing', () {
    final nav = container.read(navigationProvider.notifier);

    nav.openChannel(serverA, 10);
    nav.openChannel(serverA, 10);

    final state = container.read(navigationHistoryProvider);
    expect(state.entries, [const ChannelEntry(serverA, 10)]);
    expect(state.index, 0);
  });

  test('history_keeps_100', () {
    final nav = container.read(navigationProvider.notifier);

    for (var i = 0; i < 110; i++) {
      nav.openChannel(serverA, 1000 + i);
    }

    final state = container.read(navigationHistoryProvider);
    expect(state.entries.length, 100);
    expect(state.index, 99);
    expect(state.entries.first, const ChannelEntry(serverA, 1010));
    expect(state.entries.last, const ChannelEntry(serverA, 1109));
  });

  test('going_back_to_a_voice_channel_does_not_join', () {
    final nav = container.read(navigationProvider.notifier);
    final history = container.read(navigationHistoryProvider.notifier);

    nav.openChannel(serverA, 10); // text channel
    nav.openChannel(serverA, 12); // voice channel

    expect(container.read(voiceSessionProvider).connected, isFalse);

    history.back(); // back to text channel 10

    expect(container.read(currentChannelProvider), 10);
    expect(container.read(voiceSessionProvider).connected, isFalse);

    history.forward(); // forward to voice channel 12

    expect(container.read(currentChannelProvider), 12);
    expect(container.read(voiceSessionProvider).connected, isFalse);
  });

  test('going_back_skips_what_is_gone', () {
    final nav = container.read(navigationProvider.notifier);
    final history = container.read(navigationHistoryProvider.notifier);

    nav.openChannel(serverA, 10); // 0
    nav.openChannel(serverA, 11); // 1 (will be deleted)
    nav.openChannel(serverB, 20); // 2 (server will be removed)
    nav.openChannel(serverA, 12); // 3

    expect(container.read(navigationHistoryProvider).entries.length, 4);

    // Delete channel 11 from serverA and remove serverB from serverList
    (container.read(serverListProvider.notifier) as _TestServerListNotifier)
        .setServers([const ServerSummary(key: serverA, name: 'Server A')]);

    (container.read(serverProvider(serverA).notifier) as _TestServerNotifier)
        .setServerState(
          ServerState(
            key: serverA,
            data: _serverData(serverA, 'Server A', [
              const Channel(
                id: 10,
                kind: ChannelKind.text,
                name: 'general',
                position: 0,
              ),
              const Channel(
                id: 12,
                kind: ChannelKind.voice,
                name: 'Lounge',
                position: 2,
              ),
            ]),
          ),
        );

    final state = container.read(navigationHistoryProvider);
    expect(state.canBack, isTrue);
    expect(state.backTarget, const ChannelEntry(serverA, 10));

    history.back();

    final backState = container.read(navigationHistoryProvider);
    expect(backState.index, 0);
    expect(container.read(currentChannelProvider), 10);
    expect(backState.canForward, isTrue);
    expect(backState.forwardTarget, const ChannelEntry(serverA, 12));

    history.forward();

    final forwardState = container.read(navigationHistoryProvider);
    expect(forwardState.index, 3);
    expect(container.read(currentChannelProvider), 12);
  });

  test('the_first_resolved_channel_is_not_a_step', () {
    const serverC = 'server-c';
    final serverCNotifier = _TestServerNotifier(
      serverC,
      const ServerState(key: serverC),
    );

    final localContainer = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(store),
        serverListProvider.overrideWith(
          () => _TestServerListNotifier([
            const ServerSummary(key: serverA, name: 'Server A'),
            const ServerSummary(key: serverC, name: 'Server C'),
          ]),
        ),
        serverProvider(serverA).overrideWith(
          () => _TestServerNotifier(
            serverA,
            ServerState(
              key: serverA,
              data: _serverData(serverA, 'Server A', channelsA),
            ),
          ),
        ),
        serverProvider(serverC).overrideWith(() => serverCNotifier),
      ],
    );
    addTearDown(localContainer.dispose);
    localContainer.read(navigationHistoryProvider);

    final nav = localContainer.read(navigationProvider.notifier);
    nav.openChannel(serverA, 10);

    expect(localContainer.read(navigationHistoryProvider).entries.length, 1);

    // Open server C before its channel data arrives
    nav.openServer(serverC);

    expect(localContainer.read(navigationHistoryProvider).entries.length, 2);
    expect(
      localContainer.read(navigationHistoryProvider).entries.last,
      const ChannelEntry(serverC, null),
    );

    // Server C data arrives and resolves channel 30
    serverCNotifier.setServerState(
      ServerState(
        key: serverC,
        data: _serverData(serverC, 'Server C', [
          const Channel(
            id: 30,
            kind: ChannelKind.text,
            name: 'chat',
            position: 0,
          ),
        ]),
      ),
    );

    final state = localContainer.read(navigationHistoryProvider);
    expect(state.entries.length, 2);
    expect(state.entries.last, const ChannelEntry(serverC, 30));
  });

  test('friends_and_requests_are_steps_tabs_are_not', () {
    final nav = container.read(navigationProvider.notifier);
    final tabs = container.read(friendsTabProvider.notifier);

    nav.openFriends(); // FriendsEntry
    expect(container.read(navigationHistoryProvider).entries, [
      const FriendsEntry(),
    ]);

    tabs.select(FriendsTab.all); // Still FriendsEntry
    expect(container.read(navigationHistoryProvider).entries.length, 1);

    tabs.select(FriendsTab.pending); // Still FriendsEntry
    expect(container.read(navigationHistoryProvider).entries.length, 1);

    tabs.select(FriendsTab.requests); // RequestsEntry
    expect(container.read(navigationHistoryProvider).entries, [
      const FriendsEntry(),
      const RequestsEntry(),
    ]);

    tabs.select(FriendsTab.online); // Back to FriendsEntry
    expect(container.read(navigationHistoryProvider).entries, [
      const FriendsEntry(),
      const RequestsEntry(),
      const FriendsEntry(),
    ]);
  });
}
