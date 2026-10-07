import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/features/switcher/switcher_results.dart';

import '../../support/fixtures.dart';

const devServer = 'dev:7710';
const berlinServer = 'berlin:7710';

SwitcherChannel channel(
  String server,
  int id,
  String name, {
  ChannelKind kind = ChannelKind.text,
}) => SwitcherChannel(
  serverKey: server,
  serverName: server == devServer ? 'Opencord Dev' : 'Rust Berlin',
  channel: Channel(id: id, kind: kind, name: name),
);

final items = <SwitcherItem>[
  channel(devServer, 1, 'general'),
  channel(devServer, 2, 'dev-core'),
  channel(devServer, 3, 'design-review'),
  channel(berlinServer, 4, 'general'),
  channel(berlinServer, 5, 'meetup-planning'),
  channel(devServer, 6, 'General', kind: ChannelKind.voice),
  const SwitcherServer(serverKey: devServer, name: 'Opencord Dev'),
  const SwitcherServer(serverKey: berlinServer, name: 'Rust Berlin'),
  SwitcherMember(serverKey: devServer, member: member(kaiId, 'Kai Nakamura')),
  SwitcherMember(serverKey: devServer, member: member(miraId, 'Mira Okafor')),
];

List<String> names(List<SwitcherItem> results) => [
  for (final item in results) item.name,
];

void main() {
  test('prefix matches beat word matches, which beat contains', () {
    expect(names(rankSwitcher(items, 'de')), [
      'design-review',
      'dev-core',
      'Opencord Dev',
    ]);
  });

  test('word starts match across dashes and spaces', () {
    expect(names(rankSwitcher(items, 'plan')), ['meetup-planning']);
    expect(names(rankSwitcher(items, 'nak')), ['Kai Nakamura']);
  });

  test('a mark in front narrows the kind', () {
    expect(
      rankSwitcher(items, '#gen').map((item) => item.runtimeType).toSet(),
      {SwitcherChannel},
    );
    expect(names(rankSwitcher(items, '@mi')), ['Mira Okafor']);
    expect(names(rankSwitcher(items, '*')), ['Opencord Dev', 'Rust Berlin']);
  });

  test('ties go to channels before servers before members', () {
    final ranked = rankSwitcher([
      const SwitcherServer(serverKey: devServer, name: 'Opencord'),
      channel(devServer, 9, 'opencord'),
    ], 'open');
    expect(ranked.first, isA<SwitcherChannel>());
  });

  test('an empty query shows recent channels first', () {
    final recent = [
      (server: berlinServer, channel: 5),
      (server: devServer, channel: 2),
    ];
    final ranked = rankSwitcher(items, '', recent: recent);

    expect(names(ranked).take(2), ['meetup-planning', 'dev-core']);
  });

  test('channels of the same name stay apart by server', () {
    final general = rankSwitcher(
      items,
      'general',
    ).whereType<SwitcherChannel>().toList();
    expect(general.map((item) => item.serverName).toSet(), {
      'Opencord Dev',
      'Rust Berlin',
    });
  });
}
