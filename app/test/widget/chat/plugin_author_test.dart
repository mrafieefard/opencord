import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/chat_row_view.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/features/chat/message_rows.dart';
import 'package:opencord/ui/widgets/badges.dart';

import '../../support/pump.dart';

final _at = DateTime(2026, 10, 7, 9, 41);

class _TestServerNotifier extends ServerNotifier {
  _TestServerNotifier(this._initialData) : super('test');
  final ServerData _initialData;
  @override
  ServerState build() => ServerState(key: 'test', data: _initialData);
}

class _FakeMessageActions extends Fake implements MessageActions {
  @override
  ChannelRef get channel => (server: 'test', channel: 10);
}

ChatRowActions _testActions() => ChatRowActions(
  links: MarkdownContext(userName: (_) => null, channelName: (_) => null),
  message: _FakeMessageActions(),
  onRetry: (_) {},
  onReaction: (_, _) {},
  onJumpTo: (_) {},
  onProfile: (_) {},
  onDismissNote: (_) {},
);

ServerData _serverDataWithPluginUser({bool installed = true}) {
  const self = User(id: 1000, displayName: 'Kai');
  const info = ServerInfo(
    serverIdHex: '01',
    name: 'Opencord Dev',
    everyoneRoleId: 1,
  );
  return ServerData(
    self: self,
    info: info,
    channels: const {
      10: Channel(id: 10, kind: ChannelKind.text, name: 'general', position: 0),
    },
    roles: const {
      1: Role(
        id: 1,
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: Permissions.all,
      ),
    },
    members: {
      1000: Member(
        user: self,
        joinedAt: DateTime.fromMillisecondsSinceEpoch(0),
        roleIds: const [1],
      ),
    },
    pluginUsers: {
      9001: PluginUser(
        userId: 9001,
        pluginId: 'welcome-bot',
        name: 'Welcome Bot',
        avatarHash: '',
        installed: installed,
      ),
    },
    serverPermissions: Permissions.all,
    channelPermissions: const {},
  );
}

void main() {
  testWidgets('a_plugin_message_shows_the_plugin_name_and_badge', (
    tester,
  ) async {
    final data = _serverDataWithPluginUser(installed: true);
    final message = Message(
      id: 1,
      channelId: 10,
      authorId: 9001,
      content: 'Hello, I am a bot!',
      createdAt: _at,
    );
    final lookups = ChatLookups(data: data, messages: [message]);

    final resolved = lookups.authorOf(9001);
    expect(resolved.name, 'Welcome Bot');
    expect(resolved.isPlugin, isTrue);

    await pumpThemed(
      tester,
      surface: const Size(800, 600),
      ProviderScope(
        overrides: [
          repositoryProvider.overrideWithValue(MockRepository()),
          clockProvider.overrideWithValue(() => _at),
          serverProvider('test').overrideWith(() => _TestServerNotifier(data)),
        ],
        child: Align(
          alignment: Alignment.topLeft,
          child: ChatRowView(
            row: MessageRow(
              message,
              first: true,
              last: true,
              own: false,
              mentionsMe: false,
            ),
            lookups: lookups,
            actions: _testActions(),
            channel: data.channels[10]!,
            now: _at,
          ),
        ),
      ),
    );

    expect(find.text('Welcome Bot'), findsOneWidget);
    expect(find.byType(PluginBadge), findsOneWidget);
    expect(find.text('Plugin'), findsOneWidget);
  });

  testWidgets('an_uninstalled_plugins_message_keeps_its_name', (tester) async {
    final data = _serverDataWithPluginUser(installed: false);
    final message = Message(
      id: 2,
      channelId: 10,
      authorId: 9001,
      content: 'Old bot message from uninstalled plugin',
      createdAt: _at,
    );
    final lookups = ChatLookups(data: data, messages: [message]);

    final resolved = lookups.authorOf(9001);
    expect(resolved.name, 'Welcome Bot');
    expect(resolved.isPlugin, isTrue);

    await pumpThemed(
      tester,
      surface: const Size(800, 600),
      ProviderScope(
        overrides: [
          repositoryProvider.overrideWithValue(MockRepository()),
          clockProvider.overrideWithValue(() => _at),
          serverProvider('test').overrideWith(() => _TestServerNotifier(data)),
        ],
        child: Align(
          alignment: Alignment.topLeft,
          child: ChatRowView(
            row: MessageRow(
              message,
              first: true,
              last: true,
              own: false,
              mentionsMe: false,
            ),
            lookups: lookups,
            actions: _testActions(),
            channel: data.channels[10]!,
            now: _at,
          ),
        ),
      ),
    );

    expect(find.text('Welcome Bot'), findsOneWidget);
    expect(find.byType(PluginBadge), findsOneWidget);
    expect(find.text('Plugin'), findsOneWidget);
  });

  for (final (name, colors) in themes) {
    testWidgets('plugin message in $name', (tester) async {
      final data = _serverDataWithPluginUser(installed: true);
      final message = Message(
        id: 1,
        channelId: 10,
        authorId: 9001,
        content: 'Welcome to Opencord! Type `/help` for commands.',
        createdAt: _at,
      );
      final lookups = ChatLookups(data: data, messages: [message]);

      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(700, 120),
        ProviderScope(
          overrides: [
            clockProvider.overrideWithValue(() => _at),
            serverProvider(
              'test',
            ).overrideWith(() => _TestServerNotifier(data)),
          ],
          child: Align(
            alignment: Alignment.topLeft,
            child: ChatRowView(
              row: MessageRow(
                message,
                first: true,
                last: true,
                own: false,
                mentionsMe: false,
              ),
              lookups: lookups,
              actions: _testActions(),
              channel: data.channels[10]!,
              now: _at,
            ),
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/plugin_message_$name.png'),
      );
    });
  }

  testWidgets('a_typing_plugin_shows_its_name', (tester) async {
    final data = _serverDataWithPluginUser(installed: true);
    final typingState = TypingState({
      10: {9001: _at.add(const Duration(seconds: 5))},
    });

    await pumpThemed(
      tester,
      colors: OcColors.dark,
      surface: const Size(800, 200),
      ProviderScope(
        overrides: [
          repositoryProvider.overrideWithValue(MockRepository()),
          clockProvider.overrideWithValue(() => _at),
          serverProvider('test').overrideWith(() => _TestServerNotifier(data)),
          typingProvider(
            'test',
          ).overrideWith(() => _TestTypingNotifier('test', typingState)),
        ],
        child: Scaffold(
          body: ChatHeader(
            channel: (server: 'test', channel: 10),
            onJumpTo: (_) {},
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.textContaining('Welcome Bot is typing'), findsOneWidget);
  });
}

class _TestTypingNotifier extends TypingNotifier {
  _TestTypingNotifier(super.serverKey, this._initial);
  final TypingState _initial;
  @override
  TypingState build() => _initial;
}
