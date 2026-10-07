import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/servers/server_rail.dart';

import '../support/pump.dart';

void main() {
  for (final (name, colors) in themes) {
    testWidgets('server rail item states ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.rail,
        surface: const Size(7 * 76, 84),
        const Padding(
          padding: EdgeInsets.only(top: 6),
          child: Row(
            children: [
              RailItemView(id: 'a', name: 'Opencord Dev'),
              RailItemView(id: 'b', name: 'Hovered', hovered: true),
              RailItemView(id: 'c', name: 'Selected', selected: true),
              RailItemView(id: 'd', name: 'Unread', unread: true),
              RailItemView(
                id: 'e',
                name: 'Mentions',
                unread: true,
                mentions: 3,
              ),
              RailItemView(
                id: 'f',
                name: 'Homelab',
                connection: RailConnection.connecting,
              ),
              RailItemView(
                id: 'g',
                name: 'Failed',
                connection: RailConnection.failed,
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/rail_items_$name.png'),
      );
    });

    testWidgets('channel row states ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.sidebar,
        surface: const Size(304, 8 * 58 + 8),
        const Padding(
          padding: EdgeInsets.only(top: 4),
          child: Column(
            children: [
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'help',
                time: 'Mon',
                preview: 'Kai: Paste it under "I\'m the owner"',
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'hovered',
                time: '09:41',
                preview: 'Ava: Coffee or tea?',
                hovered: true,
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'selected',
                time: '12:04',
                preview: 'You: On it.',
                selected: true,
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'dev-core',
                time: '13:55',
                preview: 'Kai: Yes, and it shows the mismatch dialog.',
                unread: 3,
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'general',
                time: '14:28',
                preview: 'Priya: Thanks @Alex!',
                unread: 1250,
                mentions: 1,
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'off-topic',
                time: 'Tue',
                preview: 'Tomás: Water and regret.',
                unread: 12,
                muted: true,
              ),
              ChannelRowView(
                kind: ChannelKind.announcement,
                name: 'announcements',
                time: '08:30',
                preview: 'Kai: M5 starts today.',
                locked: true,
                unread: 1,
              ),
              ChannelRowView(
                kind: ChannelKind.text,
                name: 'a-very-long-channel-name-that-will-not-fit-in-the-row',
                preview: 'No messages yet',
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/channel_rows_$name.png'),
      );
    });
  }
}
