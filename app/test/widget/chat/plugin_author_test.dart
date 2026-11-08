import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/chat_row_view.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/widgets/badges.dart';

import '../../support/pump.dart';

final _at = DateTime(2026, 10, 7, 9, 41);

ServerData _serverDataWithPluginUser({bool installed = true}) {
  const self = User(
    id: 1000,
    displayName: 'Kai',
  );
  const info = ServerInfo(
    serverIdHex: '01',
    name: 'Opencord Dev',
    everyoneRoleId: 1,
  );
  return ServerData(
    self: self,
    info: info,
    channels: const {
      10: Channel(
        id: 10,
        kind: ChannelKind.text,
        name: 'general',
        position: 0,
      ),
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
  testWidgets('a_plugin_message_shows_the_plugin_name_and_badge', (tester) async {
    final data = _serverDataWithPluginUser(installed: true);
    final lookups = ChatLookups(
      data: data,
      messages: [
        Message(
          id: 1,
          channelId: 10,
          authorId: 9001,
          content: 'Hello, I am a bot!',
          createdAt: _at,
        ),
      ],
    );

    final resolved = lookups.authorOf(9001);
    expect(resolved.name, 'Welcome Bot');
    expect(resolved.isPlugin, isTrue);

    await pumpThemed(
      tester,
      surface: const Size(800, 600),
      Align(
        alignment: Alignment.topLeft,
        child: CompactMessage(
          serverKey: 'test',
          message: Message(
            id: 1,
            channelId: 10,
            authorId: 9001,
            content: 'Hello, I am a bot!',
            createdAt: _at,
          ),
          first: true,
          author: resolved.name,
          isPlugin: resolved.isPlugin,
          links: MarkdownContext(userName: (_) => null, channelName: (_) => null),
        ),
      ),
    );

    expect(find.text('Welcome Bot'), findsOneWidget);
    expect(find.byType(PluginBadge), findsOneWidget);
    expect(find.text('Plugin'), findsOneWidget);
  });

  testWidgets('an_uninstalled_plugins_message_keeps_its_name', (tester) async {
    final data = _serverDataWithPluginUser(installed: false);
    final lookups = ChatLookups(
      data: data,
      messages: [
        Message(
          id: 2,
          channelId: 10,
          authorId: 9001,
          content: 'Old bot message from uninstalled plugin',
          createdAt: _at,
        ),
      ],
    );

    final resolved = lookups.authorOf(9001);
    expect(resolved.name, 'Welcome Bot');
    expect(resolved.isPlugin, isTrue);

    await pumpThemed(
      tester,
      surface: const Size(800, 600),
      Align(
        alignment: Alignment.topLeft,
        child: CompactMessage(
          serverKey: 'test',
          message: Message(
            id: 2,
            channelId: 10,
            authorId: 9001,
            content: 'Old bot message from uninstalled plugin',
            createdAt: _at,
          ),
          first: true,
          author: resolved.name,
          isPlugin: resolved.isPlugin,
          links: MarkdownContext(userName: (_) => null, channelName: (_) => null),
        ),
      ),
    );

    expect(find.text('Welcome Bot'), findsOneWidget);
    expect(find.byType(PluginBadge), findsOneWidget);
    expect(find.text('Plugin'), findsOneWidget);
  });

  for (final (name, colors) in themes) {
    testWidgets('plugin message in $name', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(700, 120),
        Align(
          alignment: Alignment.topLeft,
          child: CompactMessage(
            serverKey: 'test',
            message: Message(
              id: 1,
              channelId: 10,
              authorId: 9001,
              content: 'Welcome to Opencord! Type `/help` for commands.',
              createdAt: _at,
            ),
            first: true,
            author: 'Welcome Bot',
            isPlugin: true,
            links: MarkdownContext(userName: (_) => null, channelName: (_) => null),
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/plugin_message_$name.png'),
      );
    });
  }
}
